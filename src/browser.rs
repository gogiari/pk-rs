//! Browser-specific proxy profiles. Commands are always argv, never shell code.
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
    net::TcpStream,
    time::timeout,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BrowserKind {
    Chrome,
    Edge,
    Firefox,
    Safari,
}

impl BrowserKind {
    pub const ALL: [Self; 4] = [Self::Chrome, Self::Edge, Self::Firefox, Self::Safari];
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "chrome" => Ok(Self::Chrome),
            "edge" => Ok(Self::Edge),
            "firefox" => Ok(Self::Firefox),
            "safari" => Ok(Self::Safari),
            _ => Err("브라우저는 chrome, edge, firefox, safari 중 하나를 지정하세요.".into()),
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Chrome => "chrome",
            Self::Edge => "edge",
            Self::Firefox => "firefox",
            Self::Safari => "safari",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Chrome => "Chrome",
            Self::Edge => "Edge",
            Self::Firefox => "Firefox",
            Self::Safari => "Safari",
        }
    }
    pub fn supported(self) -> bool {
        self != Self::Safari || env::consts::OS == "macos"
    }
    fn flatpak_id(self) -> Option<&'static str> {
        match self {
            Self::Chrome => Some("com.google.Chrome"),
            Self::Edge => Some("com.microsoft.Edge"),
            Self::Firefox => Some("org.mozilla.firefox"),
            Self::Safari => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum Launcher {
    Executable { path: String },
    Flatpak { app_id: String },
    Snap { name: String },
}

#[derive(Serialize)]
pub struct BrowserInfo {
    pub kind: BrowserKind,
    pub label: &'static str,
    pub saved: Option<Launcher>,
    pub detected: Option<Launcher>,
    pub available: bool,
    pub error: Option<String>,
    pub supported: bool,
    pub system_proxy: Option<crate::safari::SystemProxyInfo>,
}

pub fn normalize_launcher(kind: BrowserKind, launcher: Launcher) -> Result<Launcher, String> {
    if !kind.supported() {
        return Err("Safari는 macOS에서만 지원합니다.".into());
    }
    if kind == BrowserKind::Safari && !matches!(launcher, Launcher::Executable { .. }) {
        return Err("Safari는 macOS의 Safari.app 실행 파일을 지정하세요.".into());
    }
    let launcher = launcher.normalized()?;
    if kind == BrowserKind::Safari {
        safari_app(&launcher)?;
    }
    Ok(launcher)
}

fn safari_app(launcher: &Launcher) -> Result<PathBuf, String> {
    if let Launcher::Executable { path } = launcher {
        if let Some(app) = Path::new(path).ancestors().find(|path| {
            path.file_name()
                .map(|name| name == "Safari.app")
                .unwrap_or(false)
        }) {
            return Ok(app.to_owned());
        }
    }
    Err("Safari.app/Contents/MacOS/Safari 실행 파일을 지정하세요.".into())
}

fn quiet_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
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

fn executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
    }
    #[cfg(not(unix))]
    {
        path.extension()
            .map(|e| e.eq_ignore_ascii_case("exe"))
            .unwrap_or(false)
    }
}

fn resolve_program(value: &str) -> Option<PathBuf> {
    let input = crate::wsl_browser::input_path(value).ok()?;
    let path = input.as_path();
    if path.is_absolute() {
        return executable(path).then(|| path.to_owned());
    }
    // A relative path must not depend on the daemon's working directory.
    if path.components().count() != 1 {
        return None;
    }
    let search_path = env::var_os("PATH")?;
    for dir in env::split_paths(&search_path) {
        if !dir.is_absolute() {
            continue;
        }
        let candidate = dir.join(value);
        if executable(&candidate) {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let candidate = dir.join(format!("{value}.exe"));
            if executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

fn valid_package(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
}

impl Launcher {
    pub fn normalized(&self) -> Result<Self, String> {
        self.validate()?;
        match self {
            Self::Executable { path } => Ok(from_path(
                resolve_program(path).ok_or("실행 파일을 찾을 수 없습니다.")?,
            )),
            _ => Ok(self.clone()),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Executable { path } => {
                resolve_program(path).ok_or_else(|| format!("실행 파일을 찾을 수 없습니다: {path}. 절대 경로나 PATH에 있는 명령 이름을 입력하세요."))?;
            }
            Self::Flatpak { app_id } => {
                if env::consts::OS != "linux" {
                    return Err("Flatpak은 Linux에서만 사용할 수 있습니다.".into());
                }
                if !valid_package(app_id) {
                    return Err("올바른 Flatpak 앱 ID를 입력하세요.".into());
                }
                let program =
                    resolve_program("flatpak").ok_or("flatpak 명령을 찾을 수 없습니다.")?;
                if !quiet_command(program)
                    .args(["info", app_id])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
                {
                    return Err(format!("설치된 Flatpak 앱을 찾을 수 없습니다: {app_id}"));
                }
            }
            Self::Snap { name } => {
                if env::consts::OS != "linux" {
                    return Err("Snap은 Linux에서만 사용할 수 있습니다.".into());
                }
                if !valid_package(name) {
                    return Err("올바른 Snap 이름을 입력하세요.".into());
                }
                let program = resolve_program("snap").ok_or("snap 명령을 찾을 수 없습니다.")?;
                if !quiet_command(program)
                    .args(["list", name])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
                {
                    return Err(format!("설치된 Snap 앱을 찾을 수 없습니다: {name}"));
                }
            }
        }
        Ok(())
    }

    fn command(&self) -> Result<Command, String> {
        match self {
            Self::Executable { path } => {
                Ok(quiet_command(resolve_program(path).ok_or_else(|| {
                    format!("실행 파일을 찾을 수 없습니다: {path}")
                })?))
            }
            Self::Flatpak { app_id } => {
                let mut cmd = quiet_command(
                    resolve_program("flatpak").ok_or("flatpak 명령을 찾을 수 없습니다.")?,
                );
                cmd.args(["run", app_id]);
                Ok(cmd)
            }
            Self::Snap { name } => {
                let mut cmd =
                    quiet_command(resolve_program("snap").ok_or("snap 명령을 찾을 수 없습니다.")?);
                cmd.args(["run", name]);
                Ok(cmd)
            }
        }
    }
}

fn from_path(path: PathBuf) -> Launcher {
    if path.starts_with("/snap/bin") {
        Launcher::Snap {
            name: path.file_name().unwrap().to_string_lossy().into_owned(),
        }
    } else {
        Launcher::Executable {
            path: path.to_string_lossy().into_owned(),
        }
    }
}

pub fn detect(kind: BrowserKind) -> Option<Launcher> {
    if !kind.supported() {
        return None;
    }
    if crate::wsl_browser::active() {
        if let Some(launcher) = crate::wsl_browser::detect(kind) { return Some(launcher); }
    }
    let mut candidates = Vec::new();
    #[cfg(windows)]
    {
        let filename = match kind {
            BrowserKind::Chrome => "chrome.exe",
            BrowserKind::Edge => "msedge.exe",
            BrowserKind::Firefox => "firefox.exe",
            BrowserKind::Safari => return None,
        };
        // App Paths includes per-user and custom-directory installations.
        let script = r#"
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
foreach ($hive in @([Microsoft.Win32.RegistryHive]::CurrentUser, [Microsoft.Win32.RegistryHive]::LocalMachine)) {
    foreach ($view in @([Microsoft.Win32.RegistryView]::Registry64, [Microsoft.Win32.RegistryView]::Registry32)) {
        $base = [Microsoft.Win32.RegistryKey]::OpenBaseKey($hive, $view)
        try {
            $key = $base.OpenSubKey('Software\Microsoft\Windows\CurrentVersion\App Paths\' + $env:PK_BROWSER_FILENAME)
            if ($null -ne $key) {
                try { $value = $key.GetValue(''); if ($value) { [Console]::WriteLine($value) } }
                finally { $key.Dispose() }
            }
        } finally { $base.Dispose() }
    }
}
"#;
        if let Ok(output) = quiet_command("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("PK_BROWSER_FILENAME", filename)
            .stdout(Stdio::piped())
            .output()
        {
            for value in String::from_utf8_lossy(&output.stdout).lines() {
                if !value.trim().is_empty() {
                    candidates.push(PathBuf::from(value.trim().trim_matches('"')));
                }
            }
        }
        let relative = match kind {
            BrowserKind::Chrome => r"Google\Chrome\Application\chrome.exe",
            BrowserKind::Edge => r"Microsoft\Edge\Application\msedge.exe",
            BrowserKind::Firefox => r"Mozilla Firefox\firefox.exe",
            BrowserKind::Safari => return None,
        };
        for key in ["LOCALAPPDATA", "ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(dir) = env::var_os(key) {
                candidates.push(PathBuf::from(dir).join(relative));
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let app = match kind {
            BrowserKind::Chrome => "Google Chrome.app/Contents/MacOS/Google Chrome",
            BrowserKind::Edge => "Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            BrowserKind::Firefox => "Firefox.app/Contents/MacOS/firefox",
            BrowserKind::Safari => "Safari.app/Contents/MacOS/Safari",
        };
        candidates.push(PathBuf::from("/Applications").join(app));
        if kind == BrowserKind::Safari {
            candidates.push(PathBuf::from("/System/Applications").join(app));
            candidates.push(
                PathBuf::from("/System/Volumes/Preboot/Cryptexes/App/System/Applications")
                    .join(app),
            );
        }
        if let Some(home) = env::var_os("HOME") {
            candidates.push(PathBuf::from(home).join("Applications").join(app));
        }
    }
    let names: &[&str] = match kind {
        BrowserKind::Chrome => &[
            "google-chrome",
            "google-chrome-stable",
            "chrome",
            "chromium",
            "chromium-browser",
        ],
        BrowserKind::Edge => &["msedge", "microsoft-edge", "microsoft-edge-stable"],
        BrowserKind::Firefox => &["firefox"],
        BrowserKind::Safari => &[],
    };
    for name in names {
        if let Some(path) = resolve_program(name) {
            candidates.push(path);
        }
    }
    for path in candidates {
        if executable(&path) {
            // Ubuntu's Firefox package can be a transitional launcher for Snap.
            if env::consts::OS == "linux"
                && kind == BrowserKind::Firefox
                && path == Path::new("/usr/bin/firefox")
                && Path::new("/snap/firefox/current").exists()
            {
                return Some(Launcher::Snap {
                    name: "firefox".into(),
                });
            }
            return Some(from_path(path));
        }
    }
    if env::consts::OS == "linux" {
        let launcher = Launcher::Flatpak {
            app_id: kind.flatpak_id()?.into(),
        };
        if launcher.validate().is_ok() {
            return Some(launcher);
        }
        let launcher = Launcher::Snap {
            name: if kind == BrowserKind::Chrome {
                "chromium"
            } else {
                kind.id()
            }
            .into(),
        };
        if launcher.validate().is_ok() {
            return Some(launcher);
        }
    }
    None
}

pub fn list(cfg: &Config) -> Vec<BrowserInfo> {
    BrowserKind::ALL
        .into_iter()
        .map(|kind| {
            let saved = cfg.browsers.get(&kind).cloned();
            let detected = if saved.is_none() { detect(kind) } else { None };
            let result = if !kind.supported() {
                Err("Safari는 macOS에서만 지원합니다.".into())
            } else {
                saved
                    .as_ref()
                    .or(detected.as_ref())
                    .ok_or_else(|| {
                        "설치 위치를 찾지 못했습니다. 실행 위치를 직접 지정하세요.".to_string()
                    })
                    .and_then(|launcher| normalize_launcher(kind, launcher.clone()).map(|_| ()))
            };
            BrowserInfo {
                kind,
                label: kind.label(),
                saved,
                detected,
                available: result.is_ok(),
                error: result.err(),
                supported: kind.supported(),
                system_proxy: (kind == BrowserKind::Safari).then(|| crate::safari::info(cfg)),
            }
        })
        .collect()
}

pub fn profile_path(kind: BrowserKind, launcher: &Launcher) -> Result<PathBuf, String> {
    if kind == BrowserKind::Safari {
        return Err("Safari는 기존 Safari 프로필을 사용합니다.".into());
    }
    // Sandboxed packages can only read profiles inside their private directories.
    let base = match launcher {
        Launcher::Flatpak { app_id } => {
            PathBuf::from(env::var_os("HOME").ok_or("HOME 경로를 찾을 수 없습니다.")?)
                .join(".var/app")
                .join(app_id)
                .join("config/pk")
        }
        Launcher::Snap { name } => {
            PathBuf::from(env::var_os("HOME").ok_or("HOME 경로를 찾을 수 없습니다.")?)
                .join("snap")
                .join(name)
                .join("common/pk")
        }
        Launcher::Executable { path } if path.starts_with("/snap/bin/") => {
            PathBuf::from(env::var_os("HOME").ok_or("HOME 경로를 찾을 수 없습니다.")?)
                .join("snap")
                .join(Path::new(path).file_name().ok_or("Snap 이름이 없습니다.")?)
                .join("common/pk")
        }
        _ => config_path()
            .parent()
            .ok_or("PK 설정 경로를 찾을 수 없습니다.")?
            .to_owned(),
    };
    let profile = base.join("browser-profiles").join(kind.id());
    if crate::wsl_browser::windows_launcher(launcher) {
        return crate::wsl_browser::profile_path(kind, &profile);
    }
    if profile.is_absolute() {
        Ok(profile)
    } else {
        env::current_dir()
            .map(|dir| dir.join(profile))
            .map_err(|e| e.to_string())
    }
}

pub(crate) fn bypass_entries(value: &str) -> Result<Vec<String>, String> {
    let mut entries = vec!["localhost".into(), "127.0.0.1".into(), "[::1]".into()];
    for item in value.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if item.contains([';', '\n', '\r', ' ', '"', '\\', '\0'])
            || item.contains("://")
            || item.contains('<')
            || item.strip_prefix("*.").unwrap_or(item).contains('*')
        {
            return Err(format!(
                "브라우저에서 사용할 수 없는 NO_PROXY 항목입니다: {item}"
            ));
        }
        entries.push(if item.parse::<std::net::Ipv6Addr>().is_ok() {
            format!("[{item}]")
        } else {
            item.to_owned()
        });
    }
    entries.sort();
    entries.dedup();
    Ok(entries)
}

pub(crate) fn chromium_bypass(entries: &[String]) -> String {
    let mut result = entries.to_vec();
    for entry in entries {
        if !entry.contains(['*', '/', ':'])
            && !entry.starts_with('.')
            && entry.parse::<std::net::IpAddr>().is_err()
        {
            result.push(format!("*.{entry}"));
        } else if entry.starts_with('.') {
            result.push(format!("*{entry}"));
        }
    }
    result.join(";")
}

fn firefox_preferences(port: u16, entries: &[String]) -> String {
    let bypass = entries
        .iter()
        .map(|e| {
            e.strip_prefix("*.")
                .map(|s| format!(".{s}"))
                .unwrap_or_else(|| e.clone())
        })
        .collect::<Vec<_>>()
        .join(",");
    let mut prefs = String::from("// Managed by PK. Use only with the PK browser profile.\n");
    for (key, value) in [
        ("network.proxy.type", serde_json::json!(1)),
        ("network.proxy.socks", serde_json::json!("127.0.0.1")),
        ("network.proxy.socks_port", serde_json::json!(port)),
        ("network.proxy.socks_version", serde_json::json!(5)),
        ("network.proxy.socks_remote_dns", serde_json::json!(true)),
        ("network.proxy.socks5_remote_dns", serde_json::json!(true)),
        ("network.proxy.http", serde_json::json!("")),
        ("network.proxy.ssl", serde_json::json!("")),
        (
            "network.proxy.share_proxy_settings",
            serde_json::json!(false),
        ),
        ("network.proxy.no_proxies_on", serde_json::json!(bypass)),
        ("network.proxy.failover_direct", serde_json::json!(false)),
        ("network.dns.disablePrefetch", serde_json::json!(true)),
        ("network.prefetch-next", serde_json::json!(false)),
    ] {
        prefs.push_str(&format!("user_pref({:?}, {});\n", key, value));
    }
    prefs
}

fn launch_args(
    kind: BrowserKind,
    profile: &Path,
    port: u16,
    entries: &[String],
    url: &str,
) -> Vec<std::ffi::OsString> {
    if kind == BrowserKind::Safari {
        vec![url.into()]
    } else if kind == BrowserKind::Firefox {
        vec![
            "--no-remote".into(),
            "--profile".into(),
            profile.as_os_str().to_owned(),
            "--new-window".into(),
            url.into(),
        ]
    } else {
        let mut profile_arg = std::ffi::OsString::from("--user-data-dir=");
        profile_arg.push(profile);
        vec![
            profile_arg,
            "--no-first-run".into(),
            "--new-window".into(),
            format!("--proxy-server=socks5://127.0.0.1:{port}").into(),
            format!("--proxy-bypass-list={}", chromium_bypass(entries)).into(),
            // Chromium sends target hostnames to SOCKS5 for proxy-side DNS.
            // host-resolver-rules is unnecessary here and triggers a warning.
            url.into(),
        ]
    }
}

pub fn validate_url(url: &str) -> Result<(), String> {
    // URLs never become flags, profile switches, local file loads, or scripts.
    if url == "about:blank" {
        return Ok(());
    }
    if url.contains(['\r', '\n', '\0']) {
        return Err("올바른 HTTP/HTTPS 주소를 입력하세요.".into());
    }
    let uri = url
        .parse::<axum::http::Uri>()
        .map_err(|_| "올바른 HTTP/HTTPS 주소를 입력하세요.")?;
    if matches!(uri.scheme_str(), Some("http" | "https")) && uri.host().is_some() {
        Ok(())
    } else {
        Err("HTTP/HTTPS 주소만 열 수 있습니다.".into())
    }
}

pub async fn check_socks(port: u16) -> Result<(), String> {
    let result = timeout(Duration::from_secs(2), async {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await?;
        stream.write_all(&[5, 1, 0]).await?;
        let mut reply = [0; 2];
        stream.read_exact(&mut reply).await?;
        Ok::<bool, std::io::Error>(reply == [5, 0])
    })
    .await;
    if matches!(result, Ok(Ok(true))) {
        Ok(())
    } else {
        Err(format!("SOCKS5 프록시(127.0.0.1:{port})가 준비되지 않았습니다. PK 대시보드에서 터널을 먼저 연결하세요."))
    }
}

pub async fn launch(
    cfg: &Config,
    kind: BrowserKind,
    url: Option<String>,
) -> Result<Option<PathBuf>, String> {
    if !kind.supported() {
        return Err("Safari는 macOS에서만 지원합니다.".into());
    }
    let url = url.unwrap_or_else(|| "about:blank".into());
    validate_url(&url)?;
    if kind == BrowserKind::Safari {
        launch_safari(cfg, url).await?;
        return Ok(None);
    }
    let port = cfg.socks_port;
    let cfg = cfg.clone();
    let (launcher, profile, argument_profile, windows, entries) = tokio::task::spawn_blocking(move || {
        let launcher = normalize_launcher(
            kind,
            cfg.browsers
                .get(&kind)
                .cloned()
                .or_else(|| detect(kind))
                .ok_or_else(|| format!("{} 실행 위치를 직접 지정하세요.", kind.label()))?,
        )?;
        let profile = profile_path(kind, &launcher)?;
        let windows = crate::wsl_browser::windows_launcher(&launcher);
        let argument_profile = if windows { crate::wsl_browser::argument_profile(&profile)? } else { profile.clone() };
        let entries = bypass_entries(&cfg.no_proxy)?;
        Ok::<_, String>((launcher, profile, argument_profile, windows, entries))
    })
    .await
    .map_err(|e| e.to_string())??;
    // No launch or profile write before the SOCKS handshake succeeds.
    check_socks(port).await?;
    if windows {
        tokio::task::spawn_blocking(move || crate::wsl_browser::check_windows_proxy(port)).await.map_err(|error| error.to_string())??;
    }
    let result_profile = profile.clone();
    tokio::task::spawn_blocking(move || {
        fs::create_dir_all(&profile).map_err(|e| format!("프로필 폴더 생성 실패: {e}"))?;
        if kind == BrowserKind::Firefox {
            fs::write(profile.join("user.js"), firefox_preferences(port, &entries)).map_err(|e| e.to_string())?;
        }
        let mut command = launcher.command()?;
        command.args(launch_args(kind, &argument_profile, port, &entries, &url));
        let mut child = command.spawn().map_err(|e| format!("{} 실행 실패: {e}", kind.label()))?;
        std::thread::sleep(Duration::from_millis(350));
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            if !status.success() { return Err(format!("{}가 종료 코드 {status}로 실행에 실패했습니다. 실행 위치와 프로필 사용 여부를 확인하세요.", kind.label())); }
        } else {
            // Reap the browser on Unix; dropping Child alone leaves a zombie.
            std::thread::spawn(move || { let _ = child.wait(); });
        }
        Ok::<_, String>(())
    }).await.map_err(|e| e.to_string())??;
    Ok(Some(result_profile))
}

async fn launch_safari(cfg: &Config, url: String) -> Result<(), String> {
    let snapshot = cfg.clone();
    let (app, needs_http) = tokio::task::spawn_blocking(move || {
        let launcher = normalize_launcher(
            BrowserKind::Safari,
            snapshot
                .browsers
                .get(&BrowserKind::Safari)
                .cloned()
                .or_else(|| detect(BrowserKind::Safari))
                .ok_or("Safari를 찾지 못했습니다.")?,
        )?;
        let needs_http = crate::safari::inspect(&snapshot)
            .map_err(|error| format!("{error} {}", crate::safari::setup_help(&snapshot)))?;
        Ok::<_, String>((safari_app(&launcher)?, needs_http))
    })
    .await
    .map_err(|error| error.to_string())??;
    check_socks(cfg.socks_port).await?;
    if needs_http
        && !matches!(
            timeout(
                Duration::from_secs(2),
                TcpStream::connect(("127.0.0.1", cfg.http_port))
            )
            .await,
            Ok(Ok(_))
        )
    {
        return Err(format!(
            "PK HTTP 프록시(127.0.0.1:{})가 응답하지 않습니다.",
            cfg.http_port
        ));
    }
    tokio::task::spawn_blocking(move || {
        let status = quiet_command("/usr/bin/open")
            .arg("-a")
            .arg(app)
            .arg(url)
            .status()
            .map_err(|error| format!("Safari 실행 실패: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("Safari 실행 실패: {status}"))
        }
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safari_does_not_use_chromium_flags_or_create_a_proxy_profile() {
        let launcher = Launcher::Executable {
            path: "/Applications/Safari.app/Contents/MacOS/Safari".into(),
        };
        assert_eq!(
            safari_app(&launcher).unwrap(),
            Path::new("/Applications/Safari.app")
        );
        assert!(safari_app(&Launcher::Executable {
            path: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome".into()
        })
        .is_err());
        assert!(profile_path(BrowserKind::Safari, &launcher).is_err());
        assert_eq!(
            launch_args(
                BrowserKind::Safari,
                Path::new("unused"),
                1080,
                &[],
                "https://example.com"
            ),
            vec![std::ffi::OsString::from("https://example.com")]
        );
        if env::consts::OS != "macos" {
            assert!(!BrowserKind::Safari.supported());
        }
    }

    #[test]
    fn legacy_config_and_saved_launcher_round_trip() {
        let mut cfg: Config = toml::from_str("socks_port = 19080").unwrap();
        assert!(cfg.browsers.is_empty());
        cfg.browsers.insert(
            BrowserKind::Edge,
            Launcher::Executable {
                path: r"D:\다른 폴더\edge.exe".into(),
            },
        );
        cfg.browsers.insert(
            BrowserKind::Firefox,
            Launcher::Flatpak {
                app_id: "org.mozilla.firefox".into(),
            },
        );
        let loaded: Config = toml::from_str(&toml::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(cfg.browsers, loaded.browsers);
        assert_eq!(loaded.socks_port, 19080);
    }

    #[test]
    fn proxy_arguments_preserve_spaces_and_do_not_use_fixed_port() {
        let profile = Path::new("D:/한글 폴더/PK profile");
        let entries =
            bypass_entries("localhost,::1,tailscale.com,*.tailscale.com,100.64.0.0/10").unwrap();
        let args = launch_args(
            BrowserKind::Edge,
            profile,
            19080,
            &entries,
            "https://example.com/a?x=1&y=2",
        );
        assert_eq!(args[0], "--user-data-dir=D:/한글 폴더/PK profile");
        assert!(args
            .iter()
            .any(|arg| arg == "--proxy-server=socks5://127.0.0.1:19080"));
        assert_eq!(args.last().unwrap(), "https://example.com/a?x=1&y=2");
        assert!(chromium_bypass(&entries).contains("*.tailscale.com"));
        for kind in [BrowserKind::Chrome, BrowserKind::Edge] {
            let args = launch_args(kind, profile, 19080, &entries, "https://example.com");
            assert!(!args
                .iter()
                .any(|arg| arg.to_string_lossy().starts_with("--host-resolver-rules")));
            assert!(!args
                .iter()
                .any(|arg| arg.to_string_lossy().starts_with("--test-type")));
        }
        let prefs = firefox_preferences(19080, &entries);
        assert!(prefs.contains("user_pref(\"network.proxy.socks5_remote_dns\", true)"));
        assert!(prefs.contains("user_pref(\"network.proxy.socks_port\", 19080)"));
        assert!(!prefs.contains("*.tailscale.com"));
    }

    #[test]
    fn reject_flags_scripts_and_malformed_launch_settings() {
        for url in [
            "--user-data-dir=/tmp/profile",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "https://example.com\n--flag",
            "example.com",
        ] {
            assert!(validate_url(url).is_err(), "{url}");
        }
        for url in [
            "about:blank",
            "http://localhost:8080/",
            "https://example.com/?a=1&b=2",
        ] {
            assert!(validate_url(url).is_ok(), "{url}");
        }
        for package in [
            "--command=bash",
            "org.mozilla.firefox;sh",
            "firefox --private",
        ] {
            assert!(!valid_package(package));
        }
        for exceptions in ["*", "*.foo*", "example.com;*", "https://example.com"] {
            assert!(bypass_entries(exceptions).is_err());
        }
        assert!(Launcher::Executable {
            path: "not-a-real-pk-browser-12345".into()
        }
        .validate()
        .is_err());
        assert!(Launcher::Executable {
            path: "./relative/browser.exe".into()
        }
        .validate()
        .is_err());
    }

    #[tokio::test]
    async fn requires_socks_protocol_not_just_an_open_port() {
        for reply in [[5, 0], [5, 255], [72, 84]] {
            let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
                .await
                .unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut greeting = [0; 3];
                stream.read_exact(&mut greeting).await.unwrap();
                assert_eq!(greeting, [5, 1, 0]);
                stream.write_all(&reply).await.unwrap();
            });
            assert_eq!(check_socks(port).await.is_ok(), reply == [5, 0]);
            server.await.unwrap();
        }
    }
}
