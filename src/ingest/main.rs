use anyhow::Result;

use std::{
  env,
  num::NonZeroUsize,
  path::{Path, PathBuf},
};

use structopt::StructOpt;
use walkdir::{DirEntry, WalkDir};

mod parsing;

#[derive(Debug, StructOpt)]
#[structopt(name = "ingest", about = "Ingest Chatterino logs into a pgsql database")]
struct Options {
  #[structopt(short, long, env = "SCS_DATABASE_URL")]
  uri: String,
  #[structopt(short, long, env = "SCS_LOGS_DIR", parse(from_os_str))]
  logs: PathBuf,
  #[structopt(short, long, default_value = "6")]
  threads: NonZeroUsize,
}

fn walk_logs(dir: impl AsRef<Path>) -> impl Iterator<Item = (String, String, DirEntry)> {
  WalkDir::new(dir)
    .into_iter()
    .filter_map(|e| e.ok())
    .filter(|e| e.path().extension() == Some(std::ffi::OsStr::new("log")))
    .filter_map(|e| {
      e.path()
        .file_stem()
        .and_then(|v| v.to_str())
        .and_then(|v| v.split_once('-'))
        .map(|(channel, date)| (channel.to_owned(), date.to_owned()))
        .map(|(channel, date)| (channel, date, e))
    })
}

const MIN_BATCH_SIZE_TO_INSERT: usize = 400_000;
type LogFileMsg = (String, String, walkdir::DirEntry);

fn worker_thread(wid: usize, db: db::Database, rx: crossbeam_channel::Receiver<LogFileMsg>) -> Result<usize> {
  let runtime = tokio::runtime::Handle::current();
  runtime.block_on(async move {
    log::info!("[WORKER:{wid}] Listening for messages...");
    let mut cache = ahash::AHashMap::with_capacity(10); // set this to 1 million if the cache is used as the main username resolution strategy
    let mut soa_entry = db::logs::SOAEntry::new(400_000); // 56 bytes each * 400,000 = 20MB

    while let Ok((channel, date, entry)) = rx.recv() {
      let path = entry.path().display().to_string();
      log::info!("[WORKER:{wid}] Parsing {}", path);

      let channel_id = db::channels::get_or_create_channel(&db, &channel, true, &mut cache).await?;
      if let Err(e) = parsing::process_log_file(wid, &mut soa_entry, channel_id, channel, date, entry) {
        log::warn!("[WORKER:{wid}] Failed to process {path}: {e}");
      }

      log::info!("[WORKER:{wid}] Finished parsing {path}");

      let size = soa_entry.size();
      if size > MIN_BATCH_SIZE_TO_INSERT {
        const LINES_PER_SECOND: usize = 10_000;
        let instant = std::time::Instant::now();
        log::info!(
          "[WORKER:{wid}] Inserting {size} logs. This may take a while - estimating {:.3}s.",
          (size as f64 / LINES_PER_SECOND as f64)
        );
        db::logs::insert_soa_resolved_channel(&db, &mut soa_entry).await?;
        log::info!(
          "[WORKER:{wid}] {} logs inserted in {:.4}s",
          size,
          instant.elapsed().as_secs_f64()
        );
      }
    }

    log::info!("[WORKER:{wid}] Worker loop terminated. Inserting remaining logs.");
    let size = soa_entry.size();
    let instant = std::time::Instant::now();
    db::logs::insert_soa_resolved_channel(&db, &mut soa_entry).await?;
    log::info!(
      "[WORKER:{wid}] {} logs inserted in {:.4}s",
      size,
      instant.elapsed().as_secs_f64()
    );

    Ok(wid)
  })
}

#[tokio::main]
async fn main() -> Result<()> {
  if env::var("RUST_LOG").is_err() {
    env::set_var("RUST_LOG", "INFO,sqlx=WARN");
  }
  env_logger::init();

  let opts = Options::from_args_safe()?;

  log::info!("Connecting to {}", opts.uri);
  let db = db::connect(opts.uri).await?;

  log::info!("Using {} worker thread(s)", opts.threads);
  let (tx, rx) = crossbeam_channel::bounded(opts.threads.get() * 4);
  let workers = (0..opts.threads.get())
    .map(|id| {
      let rx = rx.clone();
      let db = db.clone();
      tokio::task::spawn_blocking(move || worker_thread(id + 1, db, rx))
    })
    .collect::<Vec<_>>();

  // We want the main loop to exit as soon as all worker threads die, so we need to make sure
  // that the number of receivers is exactly the same as the number of worker threads. Without
  // dropping this one, the program will deadlock on errors.
  std::mem::drop(rx);

  log::info!("Reading logs from {}", opts.logs.display());
  for (channel, date, entry) in walk_logs(opts.logs) {
    if let Err(e) = tx.send((channel, date, entry)) {
      log::error!("All worker threads appear to be dead: {e}. Exiting.");
      break;
    }
  }

  std::mem::drop(tx);
  log::info!("Finished reading logs. Waiting for workers to finish.");

  for w in workers {
    match w.await {
      Ok(Ok(wid)) => log::info!("Worker thread {} exited successfully", wid),
      Ok(Err(e)) => {
        log::error!("Worker thread exited with an error: {e}");
      }
      Err(e) => {
        log::error!("Worker thread failed to exit gracefully: {e}");
      }
    }
  }

  Ok(())
}
