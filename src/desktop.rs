//! App-scoped desktop proxy launcher. WSL crosses the Windows loopback boundary
//! through a native stdio relay; it never changes WSL or OS proxy settings.
use crate::config::{config_path, Config};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    process::Command as AsyncCommand,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    #[default]
    Auto,
    Windows,
    Wsl,
    Linux,
    Macos,
}

impl Environment {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "auto" => Ok(Self::Auto),
            "windows" => Ok(Self::Windows),
            "wsl" => Ok(Self::Wsl),
            "linux" => Ok(Self::Linux),
            "macos" | "mac" => Ok(Self::Macos),
            _ => Err("실행 환경은 auto, windows, wsl, linux, macos 중 하나를 지정하세요.".into()),
        }
    }
    fn id(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Windows => "windows",
            Self::Wsl => "wsl",
            Self::Linux => "linux",
            Self::Macos => "macos",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Auto => "현재 OS",
            Self::Windows => "Windows",
            Self::Wsl => "WSL",
            Self::Linux => "Linux",
            Self::Macos => "macOS",
        }
    }
    fn effective(self) -> Self {
        if self != Self::Auto {
            return self;
        }
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else if crate::wsl_browser::active() {
            Self::Wsl
        } else {
            Self::Linux
        }
    }
    fn supported(self) -> bool {
        match self {
            Self::Auto => true,
            Self::Windows => cfg!(windows),
            Self::Wsl => cfg!(windows) || crate::wsl_browser::active(),
            Self::Linux => cfg!(target_os = "linux"),
            Self::Macos => cfg!(target_os = "macos"),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DesktopSettings {
    pub environment: Environment,
    pub executable: Option<String>,
    pub distribution: Option<String>,
}

#[derive(Serialize)]
pub struct EnvironmentInfo {
    environment: Environment,
    label: &'static str,
    supported: bool,
}
#[derive(Serialize)]
pub struct DesktopInfo {
    pub settings: DesktopSettings,
    environments: Vec<EnvironmentInfo>,
    distributions: Vec<String>,
}

fn quiet(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}

fn wsl_distributions() -> Vec<String> {
    if !cfg!(windows) {
        return env::var("WSL_DISTRO_NAME").into_iter().collect();
    }
    let Ok(output) = quiet("wsl.exe")
        .args(["--list", "--quiet"])
        .stdout(Stdio::piped())
        .output()
    else {
        return vec![];
    };
    if !output.status.success() {
        return vec![];
    }
    let text = if output.stdout.contains(&0) {
        String::from_utf16_lossy(
            &output
                .stdout
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>(),
        )
    } else {
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    text.lines()
        .map(|line| line.trim().trim_start_matches('\u{feff}').to_owned())
        .filter(|line| !line.is_empty() && !line.starts_with("docker-desktop"))
        .collect()
}

pub fn info(cfg: &Config) -> DesktopInfo {
    DesktopInfo {
        settings: cfg.desktop.clone(),
        environments: [
            Environment::Windows,
            Environment::Wsl,
            Environment::Linux,
            Environment::Macos,
        ]
        .into_iter()
        .map(|e| EnvironmentInfo {
            environment: e,
            label: e.label(),
            supported: e.supported(),
        })
        .collect(),
        distributions: wsl_distributions(),
    }
}

pub fn normalize(mut settings: DesktopSettings) -> Result<DesktopSettings, String> {
    if !settings.environment.supported() {
        return Err(format!(
            "이 컴퓨터에서 {} 앱을 실행할 수 없습니다.",
            settings.environment.label()
        ));
    }
    settings.executable = settings
        .executable
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    settings.distribution = settings
        .distribution
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    if let Some(path) = &settings.executable {
        if path.starts_with('-') || path.contains(['\r', '\n', '\0']) {
            return Err("실행 파일 경로나 명령 이름만 입력하세요.".into());
        }
    }
    if let Some(distro) = &settings.distribution {
        if settings.environment.effective() != Environment::Wsl {
            return Err("WSL 배포판은 WSL 환경에서만 지정하세요.".into());
        }
        if !wsl_distributions().contains(distro) {
            return Err(format!("설치된 WSL 배포판을 찾을 수 없습니다: {distro}"));
        }
    }
    Ok(settings)
}

fn resolve_native(value: &str) -> Option<PathBuf> {
    let path = Path::new(value);
    if path.is_absolute() {
        return path.is_file().then(|| path.to_owned());
    }
    if path.components().count() != 1 {
        return None;
    }
    env::split_paths(&env::var_os("PATH")?)
        .filter(|dir| dir.is_absolute())
        .flat_map(|dir| [dir.join(value), dir.join(format!("{value}.exe"))])
        .find(|path| path.is_file())
}

fn native_app(settings: &DesktopSettings) -> Result<PathBuf, String> {
    if let Some(path) = &settings.executable {
        return resolve_native(path).ok_or_else(|| format!("실행 파일을 찾을 수 없습니다: {path}"));
    }
    if cfg!(windows) {
        let script = "$p = Get-AppxPackage -Name OpenAI.Codex | Select-Object -First 1; if ($p) { Join-Path $p.InstallLocation 'app/ChatGPT.exe' }";
        let output = quiet("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .stdout(Stdio::piped())
            .output()
            .map_err(|e| e.to_string())?;
        let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
        if path.is_file() {
            return Ok(path);
        }
    }
    for value in [
        "chatgpt",
        "codex-app",
        "/Applications/ChatGPT.app/Contents/MacOS/ChatGPT",
        "/Applications/Codex.app/Contents/MacOS/Codex",
    ] {
        if let Some(path) = resolve_native(value) {
            return Ok(path);
        }
    }
    Err(
        "ChatGPT/Codex 데스크톱 앱을 찾지 못했습니다. 앱을 설치하거나 실행 파일을 지정하세요."
            .into(),
    )
}

fn wsl_command(settings: &DesktopSettings) -> Command {
    let mut cmd = quiet("wsl.exe");
    if let Some(distro) = &settings.distribution {
        cmd.args(["--distribution", distro]);
    }
    cmd
}

fn wsl_path(settings: &DesktopSettings, path: &Path) -> Result<String, String> {
    let output = wsl_command(settings)
        .args(["--exec", "wslpath", "-a", "-u"])
        .arg(path)
        .stdout(Stdio::piped())
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("WSL 경로 변환에 실패했습니다.".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn linux_helper() -> Result<PathBuf, String> {
    let own = env::current_exe().map_err(|e| e.to_string())?;
    let mut roots = env::var_os("PK_NPM_ROOT")
        .map(PathBuf::from)
        .into_iter()
        .collect::<Vec<_>>();
    roots.extend(own.ancestors().skip(1).map(Path::to_owned));
    for root in roots {
        for path in [
            root.join("binaries/linux-x64/pk"),
            root.join("npm/binaries/linux-x64/pk"),
        ] {
            if path.is_file() {
                return Ok(path);
            }
        }
    }
    Err("WSL 실행에 필요한 Linux PK 바이너리가 없습니다. 전체 npm 패키지를 설치하거나 build-linux.sh로 빌드하세요.".into())
}

pub async fn launch(cfg: &Config, settings: DesktopSettings) -> Result<(), String> {
    let settings = normalize(settings)?;
    crate::browser::check_socks(cfg.socks_port).await?;
    tokio::time::timeout(
        Duration::from_secs(3),
        TcpStream::connect(("127.0.0.1", cfg.http_port)),
    )
    .await
    .map_err(|_| "PK HTTP 프록시 연결 시간이 초과되었습니다.".to_owned())?
    .map_err(|e| format!("PK HTTP 프록시 연결 실패: {e}"))?;
    let cfg = cfg.clone();
    tokio::task::spawn_blocking(move || {
        let own = env::current_exe().map_err(|e| e.to_string())?;
        let mut cmd;
        if cfg!(windows) && settings.environment.effective() == Environment::Wsl {
            let helper = wsl_path(&settings,&linux_helper()?)?;
            let relay = wsl_path(&settings,&own)?;
            // Positional arguments, never user-provided shell text.
            let script = "if [ -n \"$1\" ]; then command -v -- \"$1\"; else command -v chatgpt || command -v codex-app; fi";
            let app = wsl_command(&settings).args(["--exec","/bin/sh","-c",script,"pk-desktop",settings.executable.as_deref().unwrap_or("")]).stdout(Stdio::piped()).output().map_err(|e| e.to_string())?;
            if !app.status.success() { return Err("선택한 WSL 배포판에 ChatGPT/Codex 앱이 없습니다. 앱을 설치하거나 Linux 실행 파일을 지정하세요.".into()); }
            let app = String::from_utf8_lossy(&app.stdout).trim().to_owned();
            cmd = wsl_command(&settings);
            cmd.args(["--exec",&helper,"desktop-run-internal",&app,&cfg.http_port.to_string(),&cfg.no_proxy,"wsl",&relay]);
        } else {
            let app = native_app(&settings)?;
            cmd = quiet(&own);
            cmd.arg("desktop-run-internal").arg(app).arg(cfg.http_port.to_string()).arg(&cfg.no_proxy).arg(settings.environment.effective().id());
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        let log_dir = config_path().parent().unwrap_or(Path::new(".")).join("desktop");
        fs::create_dir_all(&log_dir).map_err(|e| e.to_string())?;
        let log = fs::OpenOptions::new().create(true).append(true).open(log_dir.join("desktop.log")).map_err(|e| e.to_string())?;
        cmd.stdout(Stdio::from(log.try_clone().map_err(|e| e.to_string())?)).stderr(Stdio::from(log));
        let mut child = cmd.spawn().map_err(|e| format!("데스크톱 실행 실패: {e}"))?;
        std::thread::sleep(Duration::from_millis(700));
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Err(format!("데스크톱 실행기가 종료되었습니다 ({status}). desktop/desktop.log를 확인하세요."));
        }
        std::thread::spawn(move || { let _ = child.wait(); });
        Ok(())
    }).await.map_err(|e| e.to_string())?
}

async fn request_header<R: tokio::io::AsyncRead + Unpin>(input: &mut R) -> Result<Vec<u8>, String> {
    let mut header = Vec::new();
    // Avoid consuming TLS bytes beyond the CONNECT headers in the stdio relay.
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() >= 65536 {
            return Err("프록시 요청 헤더가 너무 큽니다.".into());
        }
        let byte = tokio::time::timeout(Duration::from_secs(15), input.read_u8())
            .await
            .map_err(|_| "프록시 요청 시간이 초과되었습니다.".to_owned())?
            .map_err(|e| e.to_string())?;
        header.push(byte);
    }
    Ok(header)
}

fn openai_host(host: &str) -> bool {
    [
        "chatgpt.com",
        "openai.com",
        "oaistatic.com",
        "oaiusercontent.com",
    ]
    .iter()
    .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

async fn local_dns_header(header: Vec<u8>) -> Vec<u8> {
    let Some(end) = header.windows(2).position(|b| b == b"\r\n") else {
        return header;
    };
    let line = String::from_utf8_lossy(&header[..end]);
    let fields = line.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 3 || fields[0] != "CONNECT" {
        return header;
    }
    let Some((host, port)) = fields[1].rsplit_once(':') else {
        return header;
    };
    if !openai_host(host) {
        return header;
    }
    let Ok(port_number) = port.parse::<u16>() else {
        return header;
    };
    if let Ok(Ok(mut addresses)) = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::net::lookup_host((host, port_number)),
    )
    .await
    {
        if let Some(address) = addresses.find(|a| a.is_ipv4()) {
            let mut rewritten =
                format!("CONNECT {}:{} {}", address.ip(), port, fields[2]).into_bytes();
            rewritten.extend_from_slice(&header[end..]);
            return rewritten;
        }
    }
    header
}

pub async fn relay_internal(args: &[String]) -> Result<(), String> {
    let port = args
        .first()
        .ok_or("프록시 포트가 필요합니다.")?
        .parse::<u16>()
        .map_err(|e| e.to_string())?;
    // Buffer stdio reads across WSL interop while retaining pipelined TLS bytes.
    let mut input = tokio::io::BufReader::new(tokio::io::stdin());
    let header = local_dns_header(request_header(&mut input).await?).await;
    let mut upstream = TcpStream::connect(("127.0.0.1", port))
        .await
        .map_err(|e| e.to_string())?;
    upstream
        .write_all(&header)
        .await
        .map_err(|e| e.to_string())?;
    let (mut read, mut write) = upstream.into_split();
    let mut output = tokio::io::stdout();
    let upload = async {
        tokio::io::copy(&mut input, &mut write).await?;
        write.shutdown().await
    };
    let download = async {
        tokio::io::copy(&mut read, &mut output).await?;
        output.flush().await
    };
    tokio::pin!(upload, download);
    // A peer closing its response must close relay stdout even if the client
    // keeps its request stream open. Otherwise the two sides wait for EOF forever.
    tokio::select! {
        result = &mut upload => {
            result.map_err(|e: std::io::Error| e.to_string())?;
            download.await.map_err(|e: std::io::Error| e.to_string())?;
        },
        result = &mut download => { result.map_err(|e: std::io::Error| e.to_string())?; }
    }
    Ok(())
}

async fn bridge_client(
    mut client: TcpStream,
    port: u16,
    relay: Option<String>,
) -> Result<(), String> {
    if let Some(relay) = relay {
        let header = request_header(&mut client).await?;
        let mut child = AsyncCommand::new(relay)
            .args(["desktop-relay-internal", &port.to_string()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| e.to_string())?;
        let mut input = child.stdin.take().ok_or("relay stdin")?;
        let mut output = child.stdout.take().ok_or("relay stdout")?;
        input.write_all(&header).await.map_err(|e| e.to_string())?;
        input.flush().await.map_err(|e| e.to_string())?;
        let (mut read, mut write) = client.split();
        tokio::try_join!(
            async move {
                tokio::io::copy(&mut read, &mut input).await?;
                input.shutdown().await
            },
            async {
                tokio::io::copy(&mut output, &mut write).await?;
                write.shutdown().await
            }
        )
        .map_err(|e: std::io::Error| e.to_string())?;
        child.wait().await.map_err(|e| e.to_string())?;
    } else {
        let header = local_dns_header(request_header(&mut client).await?).await;
        let mut upstream = TcpStream::connect(("127.0.0.1", port))
            .await
            .map_err(|e| e.to_string())?;
        upstream
            .write_all(&header)
            .await
            .map_err(|e| e.to_string())?;
        tokio::io::copy_bidirectional(&mut client, &mut upstream)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub async fn run_internal(args: &[String]) -> Result<(), String> {
    if args.len() < 4 {
        return Err("데스크톱 실행 인자가 부족합니다.".into());
    }
    let port = args[1].parse::<u16>().map_err(|e| e.to_string())?;
    let relay = args.get(4).cloned();
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|e| e.to_string())?;
    let proxy = format!(
        "http://{}",
        listener.local_addr().map_err(|e| e.to_string())?
    );
    // App-specific state prevents an already-running unproxied app or daemon
    // from silently receiving the launch and retaining its old environment.
    let home = if relay.is_some() {
        PathBuf::from(env::var_os("HOME").ok_or("HOME이 없습니다.")?)
            .join(".local/share/pk/desktop/wsl")
    } else {
        config_path()
            .parent()
            .unwrap_or(Path::new("."))
            .join("desktop")
            .join(&args[3])
    };
    fs::create_dir_all(home.join("codex-home")).map_err(|e| e.to_string())?;
    let bypass = crate::browser::chromium_bypass(&crate::browser::bypass_entries(&args[2])?);
    let mut cmd = AsyncCommand::new(&args[0]);
    cmd.args([
        format!("--proxy-server={proxy}"),
        format!("--proxy-bypass-list={bypass}"),
        format!("--user-data-dir={}", home.join("profile").display()),
    ]);
    if cfg!(target_os = "linux") {
        cmd.arg("--ozone-platform=x11");
    }
    for key in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"] {
        cmd.env(key, &proxy);
    }
    for key in ["NO_PROXY", "no_proxy"] {
        cmd.env(key, format!("localhost,127.0.0.1,::1,{}", args[2]));
    }
    cmd.env_remove("ALL_PROXY")
        .env_remove("all_proxy")
        .env("CODEX_HOME", home.join("codex-home"));
    cmd.stdin(Stdio::null()).kill_on_drop(true);
    let mut child = cmd.spawn().map_err(|e| format!("앱 실행 실패: {e}"))?;
    println!("{} 앱 실행: {} (PK 프록시 {})", args[3], args[0], port);
    loop {
        tokio::select! {
            result = listener.accept() => { let (client,_) = result.map_err(|e|e.to_string())?; let relay=relay.clone(); tokio::spawn(async move { if let Err(e)=bridge_client(client,port,relay).await { eprintln!("앱 프록시 중계: {e}"); } }); },
            result = child.wait() => { let status=result.map_err(|e|e.to_string())?; return if status.success() { Ok(()) } else { Err(format!("앱 종료: {status}")) }; }
        }
    }
}

pub async fn cli(args: &[String]) -> Result<(), String> {
    let mut cfg = Config::load();
    if matches!(args.first().map(String::as_str), Some("list")) {
        println!(
            "{}",
            serde_json::to_string_pretty(&info(&cfg)).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    if matches!(args.first().map(String::as_str), Some("--help" | "help")) {
        println!("codex-app-proxy [auto|windows|wsl|linux|macos] [--distro 이름] [--path 실행파일]\nchatgpt-pk: 같은 데스크톱 실행 명령\npk desktop set <환경> [--distro 이름] [--path 실행파일]\npk desktop list\npk desktop reset");
        return Ok(());
    }
    if matches!(args.first().map(String::as_str), Some("reset")) {
        cfg.desktop = DesktopSettings::default();
        cfg.save()?;
        return Ok(());
    }
    let save = args.first().map(String::as_str) == Some("set");
    let args = if save { &args[1..] } else { args };
    let mut settings = cfg.desktop.clone();
    let mut index = 0;
    if let Some(value) = args.first().filter(|s| !s.starts_with('-')) {
        settings = DesktopSettings {
            environment: Environment::parse(value)?,
            ..Default::default()
        };
        index = 1;
    } else if save {
        return Err("저장할 실행 환경을 지정하세요.".into());
    }
    while index < args.len() {
        let value = args.get(index + 1).ok_or("옵션 값이 필요합니다.")?.clone();
        match args[index].as_str() {
            "--distro" => settings.distribution = Some(value),
            "--path" => settings.executable = Some(value),
            _ => return Err(format!("알 수 없는 옵션: {}", args[index])),
        }
        index += 2;
    }
    settings = normalize(settings)?;
    if save {
        cfg.desktop = settings;
        cfg.save()?;
        println!("데스크톱 실행 환경을 저장했습니다.");
    } else {
        launch(&cfg, settings).await?;
        println!("ChatGPT/Codex 데스크톱 실행 요청을 보냈습니다.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_config_keeps_automatic_desktop_selection() {
        let config: Config = toml::from_str("ssh_target = 'user@host'").unwrap();
        assert_eq!(config.desktop, DesktopSettings::default());
    }
    #[test]
    fn dns_override_only_matches_openai_domain_boundaries() {
        assert!(openai_host("chatgpt.com"));
        assert!(openai_host("auth.openai.com"));
        assert!(!openai_host("fakeopenai.com"));
        assert!(!openai_host("openai.com.attacker.example"));
    }
    #[test]
    fn executable_arguments_cannot_be_embedded_in_an_environment() {
        assert!(Environment::parse("wsl --exec anything").is_err());
        assert_eq!(Environment::parse("mac").unwrap(), Environment::Macos);
    }
    #[tokio::test]
    async fn connect_header_does_not_consume_pipelined_tls_bytes() {
        let mut input = &b"CONNECT example.com:443 HTTP/1.1\r\n\r\nTLS payload"[..];
        assert_eq!(
            request_header(&mut input).await.unwrap(),
            b"CONNECT example.com:443 HTTP/1.1\r\n\r\n"
        );
        assert_eq!(input, b"TLS payload");
        let mut buffered =
            tokio::io::BufReader::new(&b"CONNECT example.com:443 HTTP/1.1\r\n\r\nTLS payload"[..]);
        assert_eq!(
            request_header(&mut buffered).await.unwrap(),
            b"CONNECT example.com:443 HTTP/1.1\r\n\r\n"
        );
        let mut remaining = Vec::new();
        buffered.read_to_end(&mut remaining).await.unwrap();
        assert_eq!(remaining, b"TLS payload");
    }
    #[tokio::test]
    async fn native_bridge_preserves_response_after_client_half_close() {
        let upstream = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = upstream.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut socket, _) = upstream.accept().await.unwrap();
            let mut request = Vec::new();
            socket.read_to_end(&mut request).await.unwrap();
            assert_eq!(request, b"CONNECT example.com:443 HTTP/1.1\r\n\r\npayload");
            socket
                .write_all(b"HTTP/1.1 200 Connection established\r\n\r\nresponse")
                .await
                .unwrap();
        });
        let bridge = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let mut client = TcpStream::connect(bridge.local_addr().unwrap())
            .await
            .unwrap();
        let (socket, _) = bridge.accept().await.unwrap();
        let task = tokio::spawn(bridge_client(socket, port, None));
        client
            .write_all(b"CONNECT example.com:443 HTTP/1.1\r\n\r\npayload")
            .await
            .unwrap();
        client.shutdown().await.unwrap();
        let mut response = Vec::new();
        tokio::time::timeout(Duration::from_secs(3), client.read_to_end(&mut response))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            response,
            b"HTTP/1.1 200 Connection established\r\n\r\nresponse"
        );
        task.await.unwrap().unwrap();
        server.await.unwrap();
    }
}
