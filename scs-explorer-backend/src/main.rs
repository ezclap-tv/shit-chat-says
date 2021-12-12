use std::sync::Arc;
use std::{collections::HashMap, env};

use actix_cors::Cors;
use actix_web::http::header;
use actix_web::HttpResponse;
use actix_web::{
  middleware,
  web::{self, Data},
  App, Error, HttpServer,
};
use juniper::{graphql_object, FieldError, FieldResult};
use juniper_actix::{graphiql_handler, graphql_handler, playground_handler};
use tokio::sync::RwLock;

mod loaders;
mod schema;
mod v1;

const MAX_SAMPLES: usize = 16;

#[derive(Clone)]
pub struct SharedContext(std::sync::Arc<tokio::sync::RwLock<Context>>);

impl SharedContext {
  pub fn new(ctx: Context) -> Self {
    Self(Arc::new(RwLock::new(ctx)))
  }

  pub async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, Context> {
    self.0.read().await
  }

  pub async fn write(&self) -> tokio::sync::RwLockWriteGuard<'_, Context> {
    self.0.write().await
  }
}

pub struct Context {
  log_dir: std::path::PathBuf,
  model_dir: std::path::PathBuf,
  models: HashMap<String, schema::CachedModel>,
}

impl Context {
  pub fn new(log_dir: std::path::PathBuf, model_dir: std::path::PathBuf) -> Self {
    Context {
      log_dir,
      model_dir,
      models: HashMap::new(),
    }
  }

  pub fn refresh(&mut self) {}
}

impl juniper::Context for SharedContext {}

/// The read-only API methods.
pub struct SCSQueries;

#[graphql_object(context = SharedContext)]
impl SCSQueries {
  fn api_version() -> &'static str {
    "1.0"
  }

  /// Returns the list of all logged channels
  async fn channels(context: &SharedContext) -> FieldResult<Vec<schema::Channel>> {
    let log_dir = context.read().await.log_dir.clone();
    Ok(loaders::load_channel_list(log_dir).await?.values().cloned().collect())
  }

  /// Return the meta information about a particular channel if it exists
  async fn channel(
    context: &SharedContext,
    #[graphql(description = "The name of the model to retrieve")] name: String,
  ) -> FieldResult<Option<schema::Channel>> {
    let log_dir = context.read().await.log_dir.clone();
    Ok(loaders::load_channel_list(log_dir).await?.get(&name).cloned())
  }

  /// Returns the list of all available models
  async fn models(context: &SharedContext) -> FieldResult<Vec<schema::ModelInfo>> {
    let models = loaders::load_model_list_and_refresh_model_meta_if_needed(context).await?;
    Ok(models.into_values().map(|m| m.info).collect())
  }

  /// Returns the information about a model if it exists
  async fn model_info(
    context: &SharedContext,
    #[graphql(description = "The name of the model whose information should be retrieved")] name: String,
  ) -> FieldResult<schema::ModelInfo> {
    loaders::load_model_list_and_refresh_model_meta_if_needed(context).await?;
    match context.read().await.models.get(&name[..]) {
      Some(cached) => Ok(cached.info.clone()),
      _ => Err(FieldError::from(format!("Model with `{name}` wasn't found."))),
    }
  }

  /// Returns the meta-information about a model if it exists and is loaded into memory.
  /// This method will return an error in the latter case, so make sure to call load_model() prior.
  async fn model_meta(
    context: &SharedContext,
    #[graphql(description = "The name of the model whose metadata should be retrieved")] name: String,
  ) -> FieldResult<schema::ModelMeta> {
    loaders::load_model_list_and_refresh_model_meta_if_needed(context).await?;
    use_model(context, &name, |model| Ok(model.meta.clone())).await
  }

  async fn generate_text(
    context: &SharedContext,
    #[graphql(description = "The input configuration to generate text with")] input: schema::ModelInput,
  ) -> FieldResult<schema::ModelResult> {
    loaders::load_model_list_and_refresh_model_meta_if_needed(context).await?;
    use_model(context, &input.name, |loaded| {
      let seed_phrase = input.seed_phrase.clone().unwrap_or_else(String::new);
      let words = seed_phrase.split_whitespace().collect::<Vec<_>>();
      let n_outputs = input.n_outputs.unwrap_or(1).min(100).max(1);
      let max_samples = input
        .max_samples
        .map(|n| n as usize)
        .unwrap_or(MAX_SAMPLES)
        .min(32)
        .max(0);

      let mut outputs = Vec::with_capacity(n_outputs as usize);
      for _ in 0..n_outputs {
        let (response, num_samples) = match words.len() {
          0 => chain::_sample(&*loaded.model, "", max_samples),
          1 => chain::_sample(&*loaded.model, words[0], max_samples),
          _ => chain::_sample_seq(&*loaded.model, &words, max_samples),
        };
        outputs.push(schema::ModelOutput {
          text: if response.is_empty() { None } else { Some(response) },
          num_samples: num_samples as _,
        });
      }

      Ok(schema::ModelResult {
        outputs,
        max_samples: max_samples as _,
      })
    })
    .await
  }
}

async fn use_model<T, F>(context: &SharedContext, name: &str, callback: F) -> FieldResult<T>
where
  T: juniper::GraphQLValue<juniper::DefaultScalarValue>,
  F: Fn(&schema::LoadedModel) -> FieldResult<T>,
{
  match context.read().await.models.get(name) {
    Some(cached) => match cached.loaded.as_ref() {
      Some(loaded) => callback(loaded),
      _ => Err(FieldError::from(format!(
        "The model `{name}` was found but isn't loaded. Please load the model by calling load_model() first."
      ))),
    },
    _ => Err(FieldError::from(format!("Model `{name}` wasn't found."))),
  }
}

/// The mutating API methods.
pub struct SCSMutations;

#[graphql_object(context = SharedContext)]
impl SCSMutations {
  async fn load_model(
    context: &SharedContext,
    #[graphql(description = "The name of the model to retrieve")] name: String,
  ) -> FieldResult<schema::ModelMeta> {
    loaders::load_model_list_and_refresh_model_meta_if_needed(context).await?;
    let lock = context.read().await;
    if let Some(cached) = lock.models.get(&name[..]) {
      let path = cached.path.clone();
      let last_modified = cached.info.date_modified;
      let has_model = cached.loaded.as_ref().map(|model| model.meta.clone());
      std::mem::drop(lock);

      let should_reload = loaders::should_reload_model(&path, last_modified).await?;

      match (&should_reload, has_model) {
        (Some(_), _) | (_, None) => {
          let (model, meta) = loaders::load_model(&path).await?;
          let mut lock = context.write().await;

          let cached = lock
            .models
            .get_mut(&name[..])
            .ok_or_else(|| FieldError::from("Model was evicted from the cache while being loaded"))?;

          if let Some(info) = should_reload {
            cached.info = info;
          }

          cached.loaded = Some(schema::LoadedModel { model, meta });

          Ok(cached.loaded.as_ref().unwrap().meta.clone())
        }
        (None, Some(meta)) => Ok(meta),
      }
    } else {
      Err(FieldError::from(format!("Model `{name}` wasn't found.")))
    }
  }
}

/// TODO: disable these two in production
async fn graphiql_route() -> Result<HttpResponse, Error> {
  graphiql_handler("/graphql", None).await
}
async fn playground_route() -> Result<HttpResponse, Error> {
  playground_handler("/graphql", None).await
}
async fn graphql_route(
  req: actix_web::HttpRequest,
  payload: actix_web::web::Payload,
  context: web::Data<SharedContext>,
  schema: web::Data<schema::Schema>,
) -> Result<HttpResponse, Error> {
  graphql_handler(&schema, &context, req, payload).await
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
  if std::env::var("RUST_LOG").is_err() {
    env::set_var("RUST_LOG", "info");
  }
  env_logger::init();

  let log_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .join("..")
    .join("logs");
  let model_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .join("..")
    .join("models");
  let context = SharedContext::new(Context::new(log_dir.clone(), model_dir.clone()));

  let v1_ctx = v1::ctx::Context::new(
    log_dir,
    model_dir,
    db::connect("scs", "127.0.0.1", 5432, Some(("postgres", "root"))).await?,
  );

  let server = HttpServer::new(move || {
    App::new()
      .app_data(Data::new(schema::schema()))
      .app_data(Data::new(context.clone()))
      .app_data(Data::new(v1_ctx.clone()))
      .wrap(
        Cors::default()
          .allow_any_origin()
          .allowed_methods(vec!["POST", "GET"])
          .allowed_headers(vec![header::AUTHORIZATION, header::ACCEPT])
          .allowed_header(header::CONTENT_TYPE)
          .supports_credentials()
          .max_age(3600),
      )
      .wrap(middleware::Compress::default())
      .wrap(middleware::Logger::default())
      .service(
        web::resource("/graphql")
          .route(web::post().to(graphql_route))
          .route(web::get().to(graphql_route)),
      )
      // TODO: disable this in production
      .service(web::resource("/playground").route(web::get().to(playground_route)))
      // TODO: disable this in production
      .service(web::resource("/graphiql").route(web::get().to(graphiql_route)))
      .service(v1::routes())
  });
  server.bind("127.0.0.1:8080").unwrap().run().await?;

  Ok(())
}
