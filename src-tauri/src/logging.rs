//! Problems worth looking into later (warnings, errors, panics) are kept in
//! `~/Library/Logs/<app id>/murmur.log`. Everything else only shows in the terminal of
//! a dev build.

use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::Mutex;

use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::prelude::*;

const LOG_FILE: &str = "murmur.log";
/// Past this size the log moves to `murmur.log.1`, replacing the one before.
const MAX_BYTES: u64 = 1_000_000;

/// Local time, to match the meeting times people remember.
struct LocalTime;

impl FormatTime for LocalTime {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        write!(w, "{}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"))
    }
}

pub fn init(dir: &Path) {
    let file = match open(dir) {
        Ok(file) => Some(file),
        Err(e) => {
            eprintln!("murmur: could not open the log in {}: {e}", dir.display());
            None
        }
    };
    let problems = file.map(|file| {
        tracing_subscriber::fmt::layer()
            .with_writer(Mutex::new(file))
            .with_ansi(false)
            .with_timer(LocalTime)
            .with_filter(LevelFilter::WARN)
    });
    let terminal = cfg!(debug_assertions).then(|| {
        tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_timer(LocalTime)
            .with_filter(LevelFilter::INFO)
    });
    if tracing_subscriber::registry()
        .with(problems)
        .with(terminal)
        .try_init()
        .is_err()
    {
        return;
    }

    let report = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        tracing::error!("panic in thread {}: {info}", thread.name().unwrap_or("unnamed"));
        report(info);
    }));
}

/// Opens the log for appending, starting a new one when it grew too big.
fn open(dir: &Path) -> std::io::Result<File> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(LOG_FILE);
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        std::fs::rename(&path, dir.join(format!("{LOG_FILE}.1")))?;
    }
    OpenOptions::new().create(true).append(true).open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_one_old_log_when_it_grows_too_big() {
        let dir = std::env::temp_dir().join(format!("murmur-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        drop(open(&dir).unwrap());
        std::fs::write(dir.join(LOG_FILE), "small\n").unwrap();
        drop(open(&dir).unwrap());
        assert_eq!(std::fs::read_to_string(dir.join(LOG_FILE)).unwrap(), "small\n");
        assert!(!dir.join("murmur.log.1").exists());

        std::fs::write(dir.join(LOG_FILE), vec![b'x'; MAX_BYTES as usize + 1]).unwrap();
        drop(open(&dir).unwrap());
        assert_eq!(std::fs::metadata(dir.join(LOG_FILE)).unwrap().len(), 0);
        assert_eq!(
            std::fs::metadata(dir.join("murmur.log.1")).unwrap().len(),
            MAX_BYTES + 1
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
