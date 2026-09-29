use crate::config::Config;
use crate::log_info;
use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout};

pub struct TunnelManager {
    ssh_child: Arc<Mutex<Option<Child>>>,
    config: Arc<Mutex<Config>>,
    temporary_password: Arc<Mutex<Option<String>>>,
}

impl TunnelManager {
    pub fn new(config: Arc<Mutex<Config>>) -> Self {
        Self {
            ssh_child: Arc::new(Mutex::new(None)),
            config,
            temporary_password: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn connect_ssh(&self, temporary_password: Option<String>) -> Result<(), String> {
        *self.temporary_password.lock().await = temporary_password;
        self.start_ssh().await
    }

    pub async fn clear_temporary_password(&self) {
        *self.temporary_password.lock().await = None;
    }

    pub async fn stop_ssh(&self) -> Result<(), String> {
        let mut child_guard = self.ssh_child.lock().await;
        if let Some(mut existing) = child_guard.take() {
            log_info!("[Tunnel] 기존 SSH 터널 프로세스를 중지합니다.");
            let _ = existing.kill().await;
        }
        Ok(())
    }

    pub async fn start_ssh(&self) -> Result<(), String> {
        self.stop_ssh().await?;

        let cfg = self.config.lock().await.clone();
        let socks_arg = format!("127.0.0.1:{}", cfg.socks_port);

        log_info!(
            "[Tunnel] SSH 터널 프로세스를 실행합니다: ssh -D {} -N -o ServerAliveInterval=5 -o ServerAliveCountMax=3 -o ExitOnForwardFailure=yes {}",
            socks_arg, cfg.ssh_target
        );

        let mut cmd = Command::new("ssh");
        cmd.arg("-D")
            .arg(&socks_arg)
            .arg("-N")
            .arg("-o")
            .arg("ServerAliveInterval=5")
            .arg("-o")
            .arg("ServerAliveCountMax=3")
            .arg("-o")
            .arg("ExitOnForwardFailure=yes");

        // Optional SSH Key
        if let Some(key_path) = &cfg.ssh_key_path {
            let trimmed = key_path.trim();
            if !trimmed.is_empty() {
                log_info!("[Tunnel] SSH 개인키 적용: {}", trimmed);
                cmd.arg("-i").arg(trimmed);
                cmd.arg("-o").arg("IdentitiesOnly=yes");
            }
        }

        // A password used for this session stays in memory only and is never saved to Config.
        let temporary_password = self.temporary_password.lock().await.clone();
        let password = temporary_password.as_ref().or(cfg.ssh_password.as_ref());
        if let Some(password) = password {
            let trimmed = password.trim();
            if !trimmed.is_empty() {
                let current_exe = env::current_exe().unwrap_or_else(|_| PathBuf::from("pk"));
                log_info!("[Tunnel] 비밀번호로 백그라운드 인증을 수행합니다.");
                cmd.env("SSH_ASKPASS", &current_exe);
                cmd.env("SSH_ASKPASS_REQUIRE", "force");
                cmd.env("PK_ASKPASS_SECRET", trimmed);
                if env::var("DISPLAY").is_err() {
                    cmd.env("DISPLAY", ":0");
                }
                cmd.stdin(Stdio::null());
            } else {
                cmd.stdin(Stdio::inherit());
            }
        } else {
            cmd.stdin(Stdio::inherit());
        }

        cmd.arg(&cfg.ssh_target);
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());
        cmd.kill_on_drop(true);

        let child = cmd
            .spawn()
            .map_err(|e| format!("ssh 명령 실행 실패: {}", e))?;

        let mut child_guard = self.ssh_child.lock().await;
        *child_guard = Some(child);
        Ok(())
    }

    pub async fn wait_for_socks(&self, timeout_secs: u64) -> Result<(), String> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);

        while tokio::time::Instant::now() < deadline {
            {
                let mut child_guard = self.ssh_child.lock().await;
                if let Some(child) = child_guard.as_mut() {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            return Err(format!(
                                "SSH 터널 프로세스가 종료되었습니다 (종료 코드: {}). 비밀번호나 SSH 인증 설정을 확인하세요.",
                                status
                            ));
                        }
                        Ok(None) => {}
                        Err(e) => {
                            return Err(format!("SSH 프로세스 상태 점검 실패: {}", e));
                        }
                    }
                }
            }

            if self.is_socks_alive().await {
                let cfg = self.config.lock().await.clone();
                log_info!("[Tunnel] SOCKS5 프록시 준비 완료 (127.0.0.1:{})", cfg.socks_port);
                return Ok(());
            }

            sleep(Duration::from_millis(250)).await;
        }

        Err(format!(
            "SSH SOCKS5 터널이 {}초 내에 응답하지 않았습니다.",
            timeout_secs
        ))
    }

    pub async fn is_socks_alive(&self) -> bool {
        let cfg = self.config.lock().await.clone();
        let addr: SocketAddr = format!("127.0.0.1:{}", cfg.socks_port)
            .parse()
            .unwrap_or_else(|_| "127.0.0.1:1080".parse().unwrap());

        timeout(Duration::from_millis(600), TcpStream::connect(addr))
            .await
            .map(|r| r.is_ok())
            .unwrap_or(false)
    }

    pub async fn is_http_alive(&self) -> bool {
        let cfg = self.config.lock().await.clone();
        let addr: SocketAddr = format!("127.0.0.1:{}", cfg.http_port)
            .parse()
            .unwrap_or_else(|_| "127.0.0.1:3128".parse().unwrap());

        timeout(Duration::from_millis(600), TcpStream::connect(addr))
            .await
            .map(|r| r.is_ok())
            .unwrap_or(false)
    }

    pub async fn restart(&self) -> Result<(), String> {
        self.start_ssh().await?;
        self.wait_for_socks(30).await
    }
}
