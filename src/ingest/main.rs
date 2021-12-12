use anyhow::Result;
use regex::Regex;
use std::{
  env, fs,
  path::{Path, PathBuf},
};
use structopt::StructOpt;
use walkdir::{DirEntry, WalkDir};

#[derive(Debug, StructOpt)]
#[structopt(name = "ingest", about = "Ingest Chatterino logs into a pgsql database")]
struct Options {
  #[structopt(short, long, env = "INGEST_DB_URI")]
  uri: String,
  #[structopt(short, long, env = "INGEST_LOGS_DIR", parse(from_os_str))]
  logs: PathBuf,
}

fn parse_known_tz_offset(tz: &str) -> Result<&'static str> {
  Ok(match tz {
    "EDT" => "-0400",
    "EST" => "-0500",
    "UTC" => "+0000",
    _ => anyhow::bail!("Encountered unknown timezone: {}", tz),
  })
}

fn walk_logs(dir: impl AsRef<Path>) -> impl Iterator<Item = (String, String, DirEntry)> {
  WalkDir::new(dir).into_iter().filter_map(|e| e.ok()).filter_map(|e| {
    e.path()
      .file_stem()
      .and_then(|v| v.to_str())
      .and_then(|v| v.split_once('-'))
      .map(|(channel, date)| (channel.to_owned(), date.to_owned()))
      .map(|(channel, date)| (channel, date, e))
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
  let db = db::connect(("scs", "127.0.0.1", 5432, "postgres", Some("root"))).await?;

  log::info!("Reading logs from {}", opts.logs.display());
  let tz_re = Regex::new(r"# Start logging at \d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} (\w+)")?;
  let msg_re = Regex::new(r"\[(\d{2}:\d{2}:\d{2})\]  (\w+): (.*)")?;

  let mut entries = Vec::<db::logs::Entry>::new();
  for (channel, date, entry) in walk_logs(opts.logs) {
    entries.clear();
    log::info!("{} {} {}", channel, date, entry.path().display());
    let mut file_tz_offset = "+0000";
    let content = fs::read_to_string(entry.path())?;
    for line in content.split('\n') {
      if let Some(timezone) = tz_re.captures(line).and_then(|v| v.get(1).map(|v| v.as_str())) {
        file_tz_offset = parse_known_tz_offset(timezone)?;
      } else if let Some((time, chatter, message)) = msg_re
        .captures(line)
        .and_then(|v| Some((v.get(1)?.as_str(), v.get(2)?.as_str(), v.get(3)?.as_str())))
      {
        // format options: https://docs.rs/chrono/latest/chrono/format/strftime/index.html
        let log_entry = db::logs::Entry::new(
          channel.clone(),
          chatter.to_string(),
          chrono::DateTime::parse_from_str(&format!("{date} {time} {file_tz_offset}"), "%F %T %z")?
            .with_timezone(&chrono::Utc),
          message.to_string(),
        );
        /* log::info!(
          "#{} [{}] {}: {}",
          log_entry.channel(),
          log_entry.sent_at().to_rfc3339(),
          log_entry.chatter(),
          log_entry.message()
        ); */
        entries.push(log_entry);
      }
    }

    db::logs::Entry::insert_soa(&db, std::mem::take(&mut entries)).await?;
  }
  Ok(())
}
