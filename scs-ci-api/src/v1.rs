use std::process::Stdio;

use actix_web::{dev::ServiceRequest, get, post, web, HttpResponse, Scope};
use actix_web_httpauth::{extractors::bearer::BearerAuth, middleware::HttpAuthentication};
use async_stream::try_stream;
use futures::{Stream, StreamExt, TryStreamExt};

use crate::{ctx, schema, streaming::StreamLock};

use tokio::{
  io::{AsyncBufReadExt, BufReader},
  process::Command,
};

macro_rules! stream_cmd {
  ($ctx:ident,$cmd:expr) => {{
    let lock = $ctx.write_owned().await;
    let stream = execute_command($cmd);
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
macro_rules! cmd_output {
  ($cmd_output:expr) => {{
    let mut output = serde_json::to_vec($cmd_output).expect("Infallible serialization failed");
    output.push(b'\n');
    web::Bytes::from(output)
  }};
}

fn execute_command(mut cmd: Command) -> impl Stream<Item = actix_web::Result<web::Bytes>> {
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
                yield cmd_output!(&schema::CommandOutput::new(
                     line,
                    schema::OutputKind::Stdout,
                ));
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
                yield cmd_output!(&schema::CommandOutput::new(
                  line,
                    schema::OutputKind::Stderr,
                ));
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
            yield cmd_output!(&schema::CommandResult::new(
                true,
                status_line.clone(),
            ));
          }
          Err(status) => {
            let status_line = format!("command returned a non-zero exit status: {status:?}");
            yield cmd_output!(&schema::CommandResult::new(
                 false,
                 status_line.clone(),
            ));
            Err(Box::<dyn std::error::Error>::from(status_line))?;
          }
      },
      Err(e) => {
        yield cmd_output!(&schema::CommandResult::new(
            false,
            format!("Command thread panicked: {}", e.to_string()),
        ));
        Err(e)?;
      }
    };
  }
}

#[post("/up")]
async fn run_compose_up(ctx: web::Data<ctx::Context>) -> actix_web::Result<HttpResponse> {
  let cmd = ctx.read().await.compose_command(|cmd| {
    cmd.arg("up");
    cmd.arg("-d");
  });
  Ok(stream_cmd!(ctx, cmd))
}

#[post("/down")]
async fn run_compose_down(ctx: web::Data<ctx::Context>) -> actix_web::Result<HttpResponse> {
  let cmd = ctx.read().await.compose_command(|cmd| {
    cmd.arg("down");
  });
  Ok(stream_cmd!(ctx, cmd))
}

#[post("/restart")]
async fn restart(ctx: web::Data<ctx::Context>) -> actix_web::Result<HttpResponse> {
  let compose_file = ctx.read().await.config.compose_file.clone();
  let lock = ctx.write_owned().await;
  // docker-compose down
  let stream = execute_command({
    ctx::compose_command(&compose_file, |cmd| {
      cmd.arg("down");
    })
  })
  // docker-compose up -d
  .chain(execute_command({
    ctx::compose_command(&compose_file, |cmd| {
      cmd.arg("up");
      cmd.arg("-d");
    })
  }));
  let stream = terminate_on_error!(stream);

  let locked = StreamLock::chain(stream, lock);
  Ok(HttpResponse::Ok().streaming(Box::pin(locked)))
}

#[post("/deploy")]
async fn deploy(ctx: web::Data<ctx::Context>) -> actix_web::Result<HttpResponse> {
  let compose_file = ctx.read().await.config.compose_file.clone();
  let lock = ctx.write_owned().await;

  // git pull
  let stream = execute_command({
    ctx::command("git", |cmd| {
      cmd.arg("pull");
    })
  })
  // docker-compose build
  .chain(execute_command({
    ctx::compose_command(&compose_file, |cmd| {
      cmd.arg("build");
    })
  }))
  // docker-compose down
  .chain(execute_command({
    ctx::compose_command(&compose_file, |cmd| {
      cmd.arg("down");
    })
  }))
  // docker compose up -d
  .chain(execute_command({
    ctx::compose_command(&compose_file, |cmd| {
      cmd.arg("up");
      cmd.arg("-d");
    })
  }));
  let stream = terminate_on_error!(stream);

  let locked = StreamLock::chain(stream, lock);
  Ok(HttpResponse::Ok().streaming(Box::pin(locked)))
}

#[get("/configs")]
async fn configs(ctx: web::Data<ctx::Context>) -> actix_web::Result<web::Json<schema::ConfigList>> {
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

async fn token_validator(req: ServiceRequest, credentials: BearerAuth) -> actix_web::Result<ServiceRequest> {
  if let Some(ctx) = req.app_data::<web::Data<ctx::Context>>() {
    let token = credentials.token();
    if ctx.read().await.config.access_tokens.contains(token) {
      return Ok(req);
    }
    Err(AuthenticationError::InvalidCredentials.into())
  } else {
    Err(AuthenticationError::InternalError.into())
  }
}

pub fn routes() -> Scope<
  impl actix_web::dev::ServiceFactory<
    ServiceRequest,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
    Config = (),
    InitError = (),
  >,
> {
  let auth = HttpAuthentication::bearer(token_validator);
  web::scope("v1")
    .wrap(auth)
    .service(run_compose_up)
    .service(run_compose_down)
    .service(deploy)
    .service(restart)
    .service(configs)
}
