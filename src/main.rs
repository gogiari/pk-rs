mod browser;
mod browser_default;
mod cli;
mod config;
mod daemon;
mod http_proxy;
mod logger;
mod safari;
mod socks;
mod tunnel;
mod web;

use config::Config;
use std::env;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tokio::sync::Mutex;
use tunnel::TunnelManager;
use web::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 0. SSH askpass callback (OpenSSH invokes pk as SSH_ASKPASS)
    if let Ok(pw) = env::var("PK_ASKPASS_SECRET") {
        println!("{}", pw);
        return Ok(());
    }

    let args: Vec<String> = env::args().collect();
    let program_name = Path::new(&args[0])
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("pk");

    // 1. Multicall binary check (e.g. codex-proxy, grok-proxy)
    match program_name {
        "codex-proxy" => {
            cli::run_proxied_command("codex", &args[1..]);
        }
        "grok-proxy" => {
            cli::run_proxied_command("grok", &args[1..]);
        }
        "claude-proxy" => {
            cli::run_proxied_command("claude", &args[1..]);
        }
        "agy-proxy" => {
            cli::run_proxied_command("agy", &args[1..]);
        }
        "ocx-proxy" | "opencodex-proxy" => {
            cli::run_proxied_command("ocx", &args[1..]);
        }
        _ => {}
    }

    // 2. Subcommands
    // Double-clicking the release executable supplies no command-line arguments.
    let subcmd = args.get(1).map(|s| s.as_str()).unwrap_or("ui");

    match subcmd {
        "browser" => {
            browser_cli(&args[2..]).await?;
            return Ok(());
        }
        "install" => {
            cli::install_symlinks()?;
            return Ok(());
        }
        "uninstall" => {
            cli::uninstall_symlinks()?;
            return Ok(());
        }
        "codex" => {
            cli::run_proxied_command("codex", &args[2..]);
        }
        "grok" => {
            cli::run_proxied_command("grok", &args[2..]);
        }
        "claude" => {
            cli::run_proxied_command("claude", &args[2..]);
        }
        "agy" => {
            cli::run_proxied_command("agy", &args[2..]);
        }
        "ocx" => {
            cli::run_proxied_command("ocx", &args[2..]);
        }
        "log" | "logs" => {
            let lines = logger::get_recent_logs(50);
            if lines.is_empty() {
                println!("기록된 로그가 없습니다.");
            } else {
                for line in lines {
                    println!("{}", line);
                }
            }
            return Ok(());
        }
        "stop" => {
            if let Some(pid) = daemon::get_running_pid() {
                println!("==> PK Proxy Service (PID: {})를 중지합니다...", pid);
                let _ = daemon::disconnect_tunnel(Config::load().web_port);
                let _ = daemon::kill_pid(pid);
                daemon::remove_pid_file();
            }
            println!("✅ PK Proxy Service가 완전히 중지되었습니다.");
            return Ok(());
        }
        "restart" => {
            if let Some(pid) = daemon::get_running_pid() {
                println!("==> 실행 중인 PK Proxy Service (PID: {})를 중지합니다...", pid);
                let _ = daemon::disconnect_tunnel(Config::load().web_port);
                let _ = daemon::kill_pid(pid);
                daemon::remove_pid_file();
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            spawn_background_daemon(false).await?;
            return Ok(());
        }
        "status" => {
            let config = Arc::new(Mutex::new(Config::load()));
            let tunnel = TunnelManager::new(config.clone());
            let cfg = config.lock().await.clone();
            let socks_alive = tunnel.is_socks_alive().await;
            let http_alive = tunnel.is_http_alive().await;
            let daemon_pid = daemon::get_running_pid();

            println!("========================================================");
            println!("  PK Proxy Service Status");
            if let Some(pid) = daemon_pid {
                println!("  - 백그라운드 데몬: 실행 중 (PID: {})", pid);
            } else {
                println!("  - 백그라운드 데몬: 정지됨");
            }
            println!("  - SSH SOCKS5 ({}): {}", cfg.socks_port, if socks_alive { "연결됨" } else { "연결 안 됨" });
            println!("  - HTTP Proxy ({}): {}", cfg.http_port, if http_alive { "작동 중" } else { "정지됨" });
            println!("  - SSH Target: {}", cfg.ssh_target);
            if cfg.ssh_password.is_some() {
                println!("  - 비밀번호: 저장됨 (자동 로그인 가능)");
            }
            if let Some(key) = &cfg.ssh_key_path {
                println!("  - 개인키: {}", key);
            }
            println!("========================================================");
            return Ok(());
        }
        "daemon-internal" => {
            run_server_loop().await?;
        }
        "ui" => {
            spawn_background_daemon(true).await?;
        }
        "start" | "daemon" => {
            let is_foreground = args.iter().any(|a| a == "--foreground" || a == "-f");
            let force_open = args.iter().any(|a| a == "--open-browser" || a == "-o");
            if is_foreground {
                run_server_loop().await?;
            } else {
                spawn_background_daemon(force_open).await?;
            }
        }
        _ => {
            print_help();
        }
    }

    Ok(())
}

async fn spawn_background_daemon(force_open: bool) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = Config::load();
    let has_credentials = cfg.ssh_password.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false)
        || cfg.ssh_key_path.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false);
    let auto_logged_in = cfg.auto_connect && has_credentials;

    let should_open = if force_open {
        true
    } else {
        !auto_logged_in && cfg.auto_open_browser
    };
    let cfg = Config::load();

    if let Some(existing_pid) = daemon::get_running_pid() {
        println!("========================================================");
        println!("  [알림] PK Proxy Service가 이미 실행 중입니다. (PID: {})", existing_pid);
        println!("  - 웹 대시보드:  http://127.0.0.1:{}", cfg.web_port);
        println!("  - 서비스 중지:  pk stop");
        println!("  - 서비스 재시작: pk restart");
        println!("  - 상태 확인:    pk status");
        println!("========================================================");
        if should_open {
            open_browser(&format!("http://127.0.0.1:{}", cfg.web_port));
        }
        return Ok(());
    }

    let current_exe = env::current_exe()?;
    let mut cmd = Command::new(&current_exe);
    cmd.arg("daemon-internal");
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let child = cmd.spawn().map_err(|e| format!("백그라운드 프로세스 실행 실패: {}", e))?;
    let pid = child.id();
    daemon::write_pid_file(pid);

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    println!("========================================================");
    println!("  🚀 PK Proxy Service가 백그라운드에서 실행되었습니다! (PID: {})", pid);
    println!("  - 웹 대시보드:  http://127.0.0.1:{}", cfg.web_port);
    println!("  - 중지 명령어:  pk stop");
    println!("  - 상태 확인:    pk status");
    println!("  - 최근 로그:    pk logs");
    println!("========================================================");

    if should_open {
        open_browser(&format!("http://127.0.0.1:{}", cfg.web_port));
    }

    Ok(())
}

async fn run_server_loop() -> Result<(), Box<dyn std::error::Error>> {
    logger::init();
    let current_pid = std::process::id();
    daemon::write_pid_file(current_pid);

    log_info!("========================================================");
    log_info!("  PK Proxy Manager 가동 (PID: {})", current_pid);
    log_info!("========================================================");

    let config = Arc::new(Mutex::new(Config::load()));
    let tunnel = Arc::new(TunnelManager::new(config.clone()));

    let cfg_snap = config.lock().await.clone();
    let http_port = cfg_snap.http_port;
    let socks_port = cfg_snap.socks_port;
    let web_port = cfg_snap.web_port;
    let auto_connect = cfg_snap.auto_connect;

    // 1. HTTP Proxy 시작
    tokio::spawn(async move {
        if let Err(e) = http_proxy::run_http_proxy(http_port, socks_port).await {
            log_error!("[HTTP Proxy Error] {}", e);
        }
    });

    // 2. SSH Tunnel 자동 시작 (설정된 경우)
    if auto_connect {
        let t = tunnel.clone();
        tokio::spawn(async move {
            if let Err(e) = t.start_ssh().await {
                log_error!("[Warning] SSH 터널 자동 실행 실패: {}", e);
            } else {
                log_info!("[Tunnel] SOCKS5 연결을 기다리는 중입니다...");
                match t.wait_for_socks(60).await {
                    Ok(_) => log_info!("[Tunnel] SSH SOCKS5 연결이 완료되었습니다."),
                    Err(e) => log_error!("[Tunnel Error] {}", e),
                }
            }
        });
    }

    let state = AppState {
        config: config.clone(),
        tunnel: tunnel.clone(),
    };

    web::run_web_server(state, web_port).await?;
    let _ = tunnel.stop_ssh().await;
    daemon::remove_pid_file();
    Ok(())
}

fn open_browser(url: &str) {
    let cmd_paths = [
        "/mnt/c/WINDOWS/System32/cmd.exe",
        "/mnt/c/Windows/System32/cmd.exe",
    ];
    for p in &cmd_paths {
        if Path::new(p).exists() {
            let mut cmd = Command::new(p);
            cmd.args(["/c", "start", url])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if Path::new("/mnt/c").exists() {
                cmd.current_dir("/mnt/c");
            }
            let _ = cmd.spawn();
            return;
        }
    }

    if Command::new("which").arg("wslview").output().map(|o| o.status.success()).unwrap_or(false) {
        let _ = Command::new("wslview")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        return;
    }

    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("cmd")
            .args(["/c", "start", url])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("xdg-open")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}

async fn browser_cli(args: &[String]) -> Result<(), String> {
    let mut cfg = Config::load();
    match args.first().map(String::as_str) {
        None => {
            let (kind, profile) = browser_default::launch(cfg, None).await?;
            print_browser_launch(kind, profile);
        }
        Some(url) if args.len() == 1 && (url.starts_with("http://") || url.starts_with("https://") || url == "about:blank") => {
            let (kind, profile) = browser_default::launch(cfg, Some(url.into())).await?;
            print_browser_launch(kind, profile);
        }
        Some("list") if args.len() == 1 => {
            let entries = tokio::task::spawn_blocking(move || {
                let default = browser_default::info(&cfg);
                println!("기본 프록시 브라우저: {} ({})", default.effective.map(|kind| kind.label()).unwrap_or("확인 불가"),
                    if default.preferred.is_some() { "PK 웹 설정" } else { "OS 기본 브라우저" });
                if let Some(error) = default.error { println!("  {error}"); }
                browser::list(&cfg)
            }).await.map_err(|e| e.to_string())?;
            for entry in entries {
                let source = if entry.saved.is_some() { "직접 지정" } else { "자동 검색" };
                println!("{} ({source}): {}", entry.label, serde_json::to_string(&entry.saved.or(entry.detected)).unwrap());
                if let Some(error) = entry.error { println!("  {error}"); }
                if let Some(proxy) = entry.system_proxy { println!("  {}", proxy.message); }
            }
        }
        Some("set") => {
            let kind = browser::BrowserKind::parse(args.get(1).ok_or("브라우저 이름을 지정하세요.")?)?;
            let launcher = match args.get(2).map(String::as_str) {
                Some("--flatpak") if args.len() == 4 => browser::Launcher::Flatpak { app_id: args[3].clone() },
                Some("--snap") if args.len() == 4 => browser::Launcher::Snap { name: args[3].clone() },
                Some(path) if args.len() == 3 && !path.starts_with('-') => browser::Launcher::Executable { path: path.into() },
                _ => return Err("사용법: pk browser set <브라우저> <실행 파일> 또는 --flatpak <앱 ID> / --snap <이름>".into()),
            };
            let launcher = browser::normalize_launcher(kind, launcher)?;
            cfg.browsers.insert(kind, launcher);
            cfg.save()?;
            println!("{} 실행 위치를 저장했습니다.", kind.label());
        }
        Some("reset") if args.len() == 2 => {
            let kind = browser::BrowserKind::parse(&args[1])?;
            cfg.browsers.remove(&kind);
            cfg.save()?;
            println!("{} 자동 검색을 사용합니다.", kind.label());
        }
        Some("safari") if args.len() == 2 && args[1] == "--setup" => {
            println!("{}", safari::setup_help(&cfg));
        }
        Some(value) if args.len() <= 2 => {
            let kind = browser::BrowserKind::parse(value)?;
            let profile = browser::launch(&cfg, kind, args.get(1).cloned()).await?;
            print_browser_launch(kind, profile);
        }
        _ => return Err("사용법: pk browser [URL] / list / <chrome|edge|firefox|safari> [URL] / set / reset".into()),
    }
    Ok(())
}

fn print_browser_launch(kind: browser::BrowserKind, profile: Option<std::path::PathBuf>) {
    if let Some(profile) = profile { println!("{} 프록시 브라우저를 실행했습니다. 프로필: {}", kind.label(), profile.display()); }
    else { println!("Safari를 PK 시스템 프록시 설정으로 실행했습니다. 기존 Safari 프로필을 사용합니다."); }
}

fn print_help() {
    println!(
        r#"PK Proxy Manager (Rust Native)

사용법:
  pk                 웹 대시보드 실행 (더블클릭 가능)
  pk ui              웹 대시보드 실행
  pk start           프록시 서비스를 백그라운드 데몬으로 실행 (브라우저 자동 열기)
  pk start -f        포그라운드(터미널)에서 실행
  pk stop            실행 중인 백그라운드 프록시 서비스 완전 중지
  pk restart         프록시 서비스 재시작
  pk status          현재 백그라운드 데몬 및 포트 상태 점검
  pk logs            최근 프록시 로그 확인 (~/.config/pk/pk.log)
  pk install         사용자 명령 경로에 CLI 명령 등록
  pk uninstall       설치된 심볼릭 링크(바로가기) 깔끔하게 삭제
  pk browser [URL]   기본 프록시 브라우저 실행 (PK 웹 설정 또는 OS 기본값)
  pk browser list    브라우저 실행 위치 확인
  pk browser <chrome|edge|firefox|safari> [URL]  프록시 브라우저 실행
  pk browser safari --setup             macOS 시스템 프록시 설정 안내
  pk browser set <브라우저> <실행 파일>   직접 지정한 실행 위치 저장
  pk browser set <브라우저> --flatpak <앱 ID>  Linux Flatpak 지정
  pk browser set <브라우저> --snap <이름>      Linux Snap 지정
  pk browser reset <브라우저>            자동 검색으로 복원
  pk codex [args...] 프록시 환경변수가 적용된 codex 실행
  pk grok [args...]  프록시 환경변수가 적용된 grok 실행
  pk claude [args...]프록시 환경변수가 적용된 claude 실행
  pk agy [args...]   프록시 환경변수가 적용된 agy 실행
  pk ocx [args...]   프록시 환경변수가 적용된 ocx 실행

설치 후에는 브라우저 http://127.0.0.1:8253 에서 비밀번호를 입력하고
'로그인 및 프록시 연결' 버튼을 클릭하시면 됩니다."#
    );
}
