use std::fs::{create_dir_all, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

static LOGGER: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn init() {
    let log_path = crate::config::config_path()
        .parent()
        .map(|p| p.join("pk.log"))
        .unwrap_or_else(|| PathBuf::from("pk.log"));

    if let Some(parent) = log_path.parent() {
        let _ = create_dir_all(parent);
    }

    let mut guard = LOGGER.lock().unwrap();
    *guard = Some(log_path);
}

pub fn log(level: &str, msg: &str) {
    let now = SystemTime::now();
    let dt = format_timestamp(now);
    let line = format!("[{}] [{}] {}", dt, level, msg);

    match level {
        "ERROR" => eprintln!("{}", line),
        _ => println!("{}", line),
    }

    if let Ok(guard) = LOGGER.lock() {
        if let Some(path) = guard.as_ref() {
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(file, "{}", line);
            }
        }
    }
}

pub fn get_recent_logs(max_lines: usize) -> Vec<String> {
    let path = if let Ok(guard) = LOGGER.lock() {
        guard.clone()
    } else {
        None
    };

    let log_path = path.unwrap_or_else(|| {
        crate::config::config_path()
            .parent()
            .map(|p| p.join("pk.log"))
            .unwrap_or_else(|| PathBuf::from("pk.log"))
    });

    if let Ok(file) = std::fs::File::open(log_path) {
        let reader = BufReader::new(file);
        let lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();
        let start = if lines.len() > max_lines {
            lines.len() - max_lines
        } else {
            0
        };
        lines[start..].to_vec()
    } else {
        Vec::new()
    }
}

fn format_timestamp(time: SystemTime) -> String {
    let duration = time.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs();
    let total_days = secs / 86400;
    let day_secs = secs % 86400;
    let hours = day_secs / 3600;
    let mins = (day_secs % 3600) / 60;
    let s = day_secs % 60;

    let mut year = 1970;
    let mut days_left = total_days as i64;
    loop {
        let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_in_year = if is_leap { 366 } else { 365 };
        if days_left >= days_in_year {
            days_left -= days_in_year;
            year += 1;
        } else {
            break;
        }
    }
    let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let month_days = [
        31,
        if is_leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1;
    for &d in &month_days {
        if days_left >= d {
            days_left -= d;
            month += 1;
        } else {
            break;
        }
    }
    let day = days_left + 1;
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year, month, day, hours, mins, s
    )
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::logger::log("INFO", &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        $crate::logger::log("WARN", &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::logger::log("ERROR", &format!($($arg)*))
    };
}
