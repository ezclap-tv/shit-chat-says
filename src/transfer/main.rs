#[tokio::main]
async fn main() -> anyhow::Result<()> {
  if std::env::var("RUST_LOG").is_err() {
    std::env::set_var("RUST_LOG", "INFO");
  }
  scs_sentry::from_env!();
  env_logger::try_init()?;

  let instant = std::time::Instant::now();
  let url = std::env::var("SCS_DATABASE_URL").expect("SCS_DATABASE_URL must be set");
  let db = db::connect(url).await.map_err(|e| {
    log::error!("Failed to open a database connection: {}", e);
    e
  })?;

  match db::logs::transfer_raw_logs(&db).await {
    Ok(rows) => {
      log::info!(
        "Successfully transferred {} raw rows into twitch_logs in {:.4}s",
        rows,
        instant.elapsed().as_secs_f32()
      );
      Ok(())
    }
    Err(e) => {
      log::error!("Failed to transfer raw logs: {}", e);
      anyhow::bail!(e);
    }
  }
}
