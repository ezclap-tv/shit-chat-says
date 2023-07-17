use chrono::{DateTime, Utc};
use db::logs::SOAEntry;
use regex::Regex;
use std::fs;

thread_local! {
  pub static TZ_RE: regex::Regex = Regex::new(r"# Start logging at \d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} (\w+)").unwrap();
  pub static MSG_RE: regex::Regex = Regex::new(r"\[(\d{2}:\d{2}:\d{2})\]  (\w+): (.*)").unwrap();
}

pub fn process_log_file(
  wid: usize,
  soa_entry: &mut SOAEntry<String, i32>,
  channel_id: i32,
  channel: String,
  date: String,
  entry: walkdir::DirEntry,
) -> anyhow::Result<()> {
  let size_in_megabytes = entry
    .metadata()
    .map(|m| m.len() as f64 / 1024.0 / 1024.0)
    .unwrap_or(-1.0);
  let file_timestamp = detect_file_date(&date, &entry);
  let instant = std::time::Instant::now();

  log::info!(
    "[WORKER:{wid}] {} {} {} [{:.4}mb] (started)",
    channel,
    date,
    entry.path().display(),
    size_in_megabytes,
  );
  let mut file_tz_offset = None;
  let content = fs::read_to_string(entry.path())?;

  for line in content.split('\n').map(str::trim) {
    if line.is_empty() {
      continue;
    }

    // Timezone comments in the Chatterino file format
    if let Some(timezone) = TZ_RE.with(|r| r.captures(line).and_then(|v| v.get(1).map(|v| v.as_str()))) {
      if let Ok(tz) = parse_known_tz_offset(timezone) {
        file_tz_offset = Some(tz);
      }
      continue;
    }

    if let Some(record) = parse_log_line(channel_id, line, &date, file_tz_offset, file_timestamp) {
      soa_entry.add(channel_id, record.chatter, record.sent_at, record.message);
    } else if file_tz_offset.is_none() {
      log::warn!("[WORKER:{wid}] Failed to parse log line: {line}");
    }
  }

  log::info!(
    "[WORKER:{wid}] {} {} {} (collect finished in {:.4}s)",
    channel,
    date,
    entry.path().display(),
    instant.elapsed().as_secs_f64()
  );
  Ok(())
}

fn parse_log_line(
  channel_id: i32,
  line: &str,
  file_name_date: &str,
  file_tz_offset: Option<&str>,
  file_timestamp: DateTime<Utc>,
) -> Option<db::logs::Entry<String, i32>> {
  if let Some(tz) = file_tz_offset {
    // Chatterino log format
    if let Some((time, chatter, message)) = MSG_RE.with(|r| {
      r.captures(line)
        .and_then(|v| Some((v.get(1)?.as_str(), v.get(2)?.as_str(), v.get(3)?.as_str())))
    }) {
      return chrono::DateTime::parse_from_str(&format!("{file_name_date} {time} {tz}"), "%F %T %z")
        .map(|sent_at| {
          db::logs::Entry::new(
            channel_id,
            chatter.to_owned(),
            sent_at.with_timezone(&chrono::Utc),
            message.to_string(),
          )
        })
        .ok();
    }
    return None;
  }

  // SCS log format, either v1 (w/o timestamp) or v2 (w/ timestamp)
  let (head, rest) = line.split_once(',')?;
  let mut sent_at = file_timestamp;
  let (chatter, message) = if let Ok(timestamp) = DateTime::parse_from_rfc3339(head) {
    sent_at = timestamp.into();
    rest.split_once(',')?
  } else {
    (head, rest)
  };

  let record = db::logs::Entry::new(channel_id, chatter.to_string(), sent_at, message.to_string());
  if record.chatter.len() > 50 {
    log::warn!("Incorrectly parsed a log line. Either this is a bug or the log file is not in the correct format. Result: {record:?}");
    return None;
  }

  Some(record)
}

fn detect_file_date(date: &str, entry: &walkdir::DirEntry) -> DateTime<Utc> {
  if let Ok(file_date) = DateTime::parse_from_str(date, "%Y-%m-%d") {
    return file_date.into();
  }

  let metadata = entry.metadata().ok();
  let created_at = metadata.as_ref().and_then(|m| m.created().ok());
  created_at
    .or_else(|| metadata.and_then(|m| m.modified().ok()))
    .map(DateTime::<Utc>::from)
    .unwrap_or_else(chrono::Utc::now)
}

fn parse_known_tz_offset(tz: &str) -> anyhow::Result<&'static str> {
  Ok(match tz {
    "EDT" => "-0400",
    "EST" => "-0500",
    "UTC" => "+0000",
    _ => anyhow::bail!("Encountered unknown timezone: {}", tz),
  })
}
