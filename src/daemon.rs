use crate::config::config_path;
use std::fs;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
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

pub fn disconnect_tunnel(web_port: u16) -> Result<(), String> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, web_port));
    let mut stream = TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(750))
        .map_err(|e| format!("Cannot reach dashboard: {e}"))?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(std::time::Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;
    stream
        .write_all(b"POST /api/tunnel/disconnect HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .map_err(|e| format!("Cannot request tunnel shutdown: {e}"))?;
    let mut response = [0u8; 32];
    let count = stream
        .read(&mut response)
        .map_err(|e| format!("Cannot confirm tunnel shutdown: {e}"))?;
    if response[..count].starts_with(b"HTTP/1.1 200") {
        Ok(())
    } else {
        Err("Dashboard did not confirm tunnel shutdown".to_string())
    }
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
