use std::process::Stdio;

use actix_web::{dev::ServiceRequest, get, post, web, HttpResponse};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use async_stream::try_stream;
use futures::{Stream, StreamExt, TryStreamExt};

use crate::{ctx, schema, streaming::StreamLock};

use tokio::{
  io::{AsyncBufReadExt, BufReader},
  process::Command,
};

macro_rules! ensure_unlocked {
  ($ctx:ident, $cmd_name:expr) => {{
    if let Some(mut lock) = $ctx.try_write() {
      lock.set_command($cmd_name)
    } else {
      return Ok(HttpResponse::new(actix_http::StatusCode::PRECONDITION_FAILED));
    }
  }};
}

macro_rules! stream_cmd {
  ($ctx:ident,$cmd:expr, $sink:expr) => {{
    let lock = $ctx.read_owned().await;
    let stream = execute_command($cmd, $sink);
    let locked = $crate::streaming::StreamLock::chain(stream, lock);
    HttpResponse::Ok().streaming(Box::pin(locked))
  }};
}
macro_rules! terminate_on_error {
  ($stream:expr) => {{
    $stream
      .inspect(|res| {
        if let Err(e) = res {
          log::error!("command failed: {}", e);
        }
      })
      .take_while(|res| futures::future::ready(res.is_ok()))
  }};
}

#[macro_export]
macro_rules! cmd_output {
  ($sink:ident, $cmd_output:expr) => {{
    let cmd_output = $cmd_output;
    $sink
      .try_send(cmd_output.clone().into())
      .expect("The receiver is destroyed on program exit so this can't fail");
    let mut output = serde_json::to_vec(cmd_output).expect("Infallible serialization failed");
    output.push(b'\n');
    actix_web::web::Bytes::from(output)
  }};
}

fn execute_command(mut cmd: Command, sink: ctx::Sink) -> impl Stream<Item = actix_web::Result<web::Bytes>> {
  try_stream! {
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().unwrap();

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    // Ensure the child process is spawned in the runtime so it can
    // make progress on its own while we await for any output.
    let handle = tokio::spawn(async move {
      let status = child.wait().await.expect("child process encountered an error");
      log::info!("child status was: {}", status);
      if status.success() { Ok(status) } else { Err(status) }
    });

    let mut stdout_exhausted = false;
    let mut stderr_exhausted = false;

    while !stdout_exhausted || !stderr_exhausted {
      tokio::select! {
        stdout_line = stdout_reader.next_line() => {
          match stdout_line {
            Ok(Some(line)) => {
                println!("{}", line);
                yield cmd_output!(sink, &schema::CommandOutput {
                    output: line,
                    output_kind: schema::OutputKind::Stdout,
                });
            },
            Ok(None) => {
              stdout_exhausted = true;
            }
            Err(e) => {
              if !stdout_exhausted {
                log::error!("error reading stdout: {}", e);
              }
              stdout_exhausted = true;
            }
          }
        }
        stderr_line = stderr_reader.next_line() => {
          match stderr_line {
              Ok(Some(line)) => {
                eprintln!("{}", line);
                yield cmd_output!(sink, &schema::CommandOutput {
                    output: line,
                    output_kind: schema::OutputKind::Stderr,
                });
              },
              Ok(None) => {
                stderr_exhausted = true;
              }
              Err(e) => {
                if !stderr_exhausted {
                  log::error!("error reading stderr: {}", e);
                }
                stderr_exhausted = true;
              }
          }
        }
      }
    }

    let join_res = handle.await.map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()));
    match join_res {
      Ok(cmd_res) => match cmd_res {
          Ok(status) => {
            let status_line = format!("command returned successfully: {status:?}");
            yield cmd_output!(sink, &schema::CommandResult {
                is_success: true,
                status_line: status_line.clone(),
            });
          }
          Err(status) => {
            let status_line = format!("command returned a non-zero exit status: {status:?}");
            yield cmd_output!(sink, &schema::CommandResult {
                is_success: false,
                status_line: status_line.clone(),
            });
            Err(Box::<dyn std::error::Error>::from(status_line))?;
          }
      },
      Err(e) => {
        yield cmd_output!(sink, &schema::CommandResult {
            is_success: false,
            status_line: format!("Command thread panicked: {}", e.to_string()),
        });
        Err(e)?;
      }
    };
  }
}

pub async fn capture_output(mut cmd: Command) -> actix_web::Result<String> {
  log::info!("running {:?}", cmd);
  let output = cmd.output().await.map_err(|e| {
    log::error!("failed to run {:?}: {}", cmd, e);
    actix_web::error::ErrorInternalServerError(e)
  })?;

  let content = String::from_utf8(output.stdout);
  if output.status.success() && content.is_ok() {
    Ok(content.unwrap())
  } else {
    Err(actix_web::error::ErrorInternalServerError(format!(
      "Failed to run {cmd:?}"
    )))
  }
}

pub async fn get_services(ctx: web::Data<ctx::Context>) -> actix_web::Result<Vec<schema::Service>> {
  let lock = ctx.read().await;
  let list_names = lock.compose_command(|a| {
    a.arg("ps");
    a.arg("--services");
  });
  let list_ids = lock.compose_command(|a| {
    a.arg("ps");
    a.arg("-q");
  });
  std::mem::drop(lock);

  let running_images = ctx::command("docker", |a| {
    a.arg("ps");
    a.arg("-q");
    a.arg("--no-trunc");
  });

  let names = capture_output(list_names).await?;
  let ids = capture_output(list_ids).await?;
  let running_image_ids = capture_output(running_images).await?;
  let running_image_ids = running_image_ids.lines().collect::<std::collections::HashSet<_>>();
  Ok(
    names
      .lines()
      .zip(ids.lines())
      .map(|(name, id)| schema::Service {
        name: name.to_string(),
        is_running: running_image_ids.contains(id),
      })
      .collect::<Vec<_>>(),
  )
}

#[get("/services")]
pub async fn services(ctx: web::Data<ctx::Context>) -> actix_web::Result<HttpResponse> {
  Ok(HttpResponse::Ok().json(get_services(ctx).await?))
}

#[post("/service/{name}/{command}")]
pub async fn manage_service(
  ctx: web::Data<ctx::Context>,
  path: web::Path<(String, String)>,
) -> actix_web::Result<HttpResponse> {
  let (name, command) = path.into_inner();
  if !["stop", "start"].contains(&&command[..]) {
    return Ok(
      HttpResponse::new(actix_http::StatusCode::BAD_REQUEST).set_body(actix_http::body::BoxBody::new(
        serde_json::json!({
          "error": "Invalid command",
          "command": name,
          "commands": ["start", "stop"]
        })
        .to_string(),
      )),
    );
  }

  let services_ = get_services(ctx.clone()).await?;

  if !services_.iter().any(|s| s.name == name) {
    return Ok(
      HttpResponse::new(actix_http::StatusCode::BAD_REQUEST).set_body(actix_http::body::BoxBody::new(
        serde_json::json!({
          "error": "Service not found",
          "name": name,
          "services": services_
        })
        .to_string(),
      )),
    );
  }

  let sink = ensure_unlocked!(ctx, format!("{} {}", command, name));
  let cmd = ctx.read().await.compose_command(move |cmd| {
    cmd.arg(command.clone());
    cmd.arg(name.clone());
  });
  Ok(stream_cmd!(ctx, cmd, sink))
}

#[post("/up")]
pub async fn run_compose_up(ctx: web::Data<ctx::Context>) -> actix_web::Result<HttpResponse> {
  let sink = ensure_unlocked!(ctx, "up");
  let cmd = ctx.read().await.compose_command(|cmd| {
    cmd.arg("up");
    cmd.arg("-d");
  });
  Ok(stream_cmd!(ctx, cmd, sink))
}

#[post("/down")]
pub async fn run_compose_down(ctx: web::Data<ctx::Context>) -> actix_web::Result<HttpResponse> {
  let sink = ensure_unlocked!(ctx, "down");
  let cmd = ctx.read().await.compose_command(|cmd| {
    cmd.arg("down");
  });
  Ok(stream_cmd!(ctx, cmd, sink))
}

#[post("/restart")]
pub async fn restart(ctx: web::Data<ctx::Context>) -> actix_web::Result<HttpResponse> {
  let sink = ensure_unlocked!(ctx, "restart");
  let compose_file = ctx.read().await.config.compose_file.clone();
  let lock = ctx.read_owned().await;
  // docker-compose down
  let stream = execute_command(
    ctx::compose_command(&compose_file, |cmd| {
      cmd.arg("down");
    }),
    sink.clone(),
  )
  // docker-compose up -d
  .chain(execute_command(
    ctx::compose_command(&compose_file, |cmd| {
      cmd.arg("up");
      cmd.arg("-d");
    }),
    sink,
  ));
  let stream = terminate_on_error!(stream);

  let locked = StreamLock::chain(stream, lock);
  Ok(HttpResponse::Ok().streaming(Box::pin(locked)))
}

#[post("/deploy")]
pub async fn deploy(ctx: web::Data<ctx::Context>) -> actix_web::Result<HttpResponse> {
  let sink = ensure_unlocked!(ctx, "deploy");
  let compose_file = ctx.read().await.config.compose_file.clone();
  let lock = ctx.read_owned().await;

  // git pull
  let stream = execute_command(
    ctx::command("git", |cmd| {
      cmd.arg("pull");
    }),
    sink.clone(),
  )
  // docker-compose build
  .chain(execute_command(
    ctx::compose_command(&compose_file, |cmd| {
      cmd.arg("build");
    }),
    sink.clone(),
  ))
  // docker-compose down
  .chain(execute_command(
    ctx::compose_command(&compose_file, |cmd| {
      cmd.arg("down");
    }),
    sink.clone(),
  ))
  // docker compose up -d
  .chain(execute_command(
    ctx::compose_command(&compose_file, |cmd| {
      cmd.arg("up");
      cmd.arg("-d");
    }),
    sink.clone(),
  ));
  let stream = terminate_on_error!(stream);

  let locked = StreamLock::chain(stream, lock);
  Ok(HttpResponse::Ok().streaming(Box::pin(locked)))
}

#[get("/configs")]
pub async fn configs(ctx: web::Data<ctx::Context>) -> actix_web::Result<web::Json<schema::ConfigList>> {
  let lock = ctx.read().await;
  let config_folder = lock.config.project_source_folder.join("config");
  let ci_api_config = lock.config_path.clone();
  std::mem::drop(lock);

  log::info!("{}", config_folder.display());

  let mut configs = Vec::with_capacity(3);
  let mut entries = async_fs::read_dir(&config_folder).await?;
  while let Some(entry) = entries.try_next().await? {
    let path = match entry.path().canonicalize() {
      Ok(path) => path,
      Err(e) => {
        log::error!("failed to resolve a path: {}", e);
        continue;
      }
    };

    // Skip directories, non-json files, example configs, and the CI config with secrets.
    let name = path.to_string_lossy();
    if path.is_dir()
      || path.extension() != Some(std::ffi::OsStr::new("json"))
      || name.ends_with("example.json")
      || path == ci_api_config
    {
      continue;
    }

    configs.push(schema::SCSConfig {
      name: path.file_name().unwrap().to_string_lossy().into_owned(),
      contents: async_fs::read_to_string(&path).await?,
    });
  }

  Ok(web::Json(schema::ConfigList { configs }))
}

#[get("/is_running")]
pub async fn is_running(ctx: web::Data<ctx::Context>) -> actix_web::Result<web::Json<bool>> {
  Ok(web::Json(ctx.try_write().is_none()))
}

#[get("/last_command")]
pub async fn last_command(ctx: web::Data<ctx::Context>) -> actix_web::Result<web::Json<schema::LastCommand>> {
  let in_progress = ctx.try_write().is_none();
  let lock = ctx.read().await;
  let last_command = lock.last_command.clone();
  let command_output = lock.get_log_history().await;
  Ok(web::Json(schema::LastCommand {
    in_progress,
    last_command,
    command_output,
  }))
}

#[derive(Debug)]
enum AuthenticationError {
  InternalError,
  InvalidCredentials,
}
impl std::fmt::Display for AuthenticationError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
      f,
      "{}",
      match self {
        AuthenticationError::InternalError => "internal error: failed to obtain the token list",
        AuthenticationError::InvalidCredentials => "invalid credentials",
      }
    )
  }
}
impl actix_web::ResponseError for AuthenticationError {
  fn status_code(&self) -> actix_http::StatusCode {
    match self {
      AuthenticationError::InternalError => actix_http::StatusCode::INTERNAL_SERVER_ERROR,
      AuthenticationError::InvalidCredentials => actix_http::StatusCode::FORBIDDEN,
    }
  }

  fn error_response(&self) -> HttpResponse {
    actix_web::HttpResponseBuilder::new(self.status_code())
      .insert_header((actix_http::header::CONTENT_TYPE, "text/html; charset=utf-8"))
      .body(self.to_string())
  }
}

pub async fn token_validator(
  req: ServiceRequest,
  credentials: BearerAuth,
) -> Result<ServiceRequest, (actix_web::Error, ServiceRequest)> {
  if let Some(ctx) = req.app_data::<web::Data<ctx::Context>>() {
    let token = credentials.token();
    if ctx.read().await.config.access_tokens.contains(token) {
      return Ok(req);
    }
    Err((AuthenticationError::InvalidCredentials.into(), req))
  } else {
    Err((AuthenticationError::InternalError.into(), req))
  }
}
