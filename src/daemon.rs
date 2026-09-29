use crate::config::config_path;
use std::fs;
use std::path::PathBuf;
#[cfg(unix)]
use std::path::Path;
use std::process::{Command, Stdio};

pub fn pid_file_path() -> PathBuf {
    config_path()
        .parent()
        .map(|p| p.join("pk.pid"))
        .unwrap_or_else(|| PathBuf::from("pk.pid"))
}
pub fn get_running_pid() -> Option<u32> {
    let path = pid_file_path();
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(pid) = content.trim().parse::<u32>() {
            if is_pid_alive(pid) {
                return Some(pid);
            } else {
                let _ = fs::remove_file(&path);
            }
        }
    }
    None
}

pub fn write_pid_file(pid: u32) {
    let path = pid_file_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&path, pid.to_string());
}

pub fn remove_pid_file() {
    let _ = fs::remove_file(pid_file_path());
}

pub fn is_pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        if Path::new(&format!("/proc/{}", pid)).exists() {
            return true;
        }
        Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        let output = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid)])
            .output();
        if let Ok(out) = output {
            let s = String::from_utf8_lossy(&out.stdout);
            s.contains(&pid.to_string())
        } else {
            false
        }
    }
}

pub fn kill_pid(pid: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .arg(pid.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        for _ in 0..15 {
            std::thread::sleep(std::time::Duration::from_millis(150));
            if !is_pid_alive(pid) {
                return Ok(());
            }
        }

        let _ = Command::new("kill")
            .arg("-9")
            .arg(pid.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        Ok(())
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        Ok(())
    }
}
