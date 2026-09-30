use crate::browser::{self, BrowserKind, Launcher};
use crate::config::Config;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
pub struct DefaultBrowserInfo {
    pub wsl: bool,
    pub preferred: Option<BrowserKind>,
    pub system: Option<BrowserKind>,
    pub system_error: Option<String>,
    pub effective: Option<BrowserKind>,
    pub error: Option<String>,
}

struct SystemBrowser {
    kind: BrowserKind,
    launcher: Option<Launcher>,
}

fn unknown(identity: &str) -> String {
    format!("OS 기본 브라우저({identity})는 PK 프록시 실행을 지원하지 않습니다. PK 웹에서 Chrome, Edge, Firefox 또는 macOS Safari를 선택하세요.")
}

fn select(
    cfg: &mut Config,
    system: impl FnOnce() -> Result<SystemBrowser, String>,
) -> Result<BrowserKind, String> {
    if let Some(kind) = cfg.default_browser {
        return if kind.supported() {
            Ok(kind)
        } else {
            Err("Safari는 macOS에서만 실행할 수 있습니다.".into())
        };
    }
    let system = system()?;
    // A manually saved executable has priority over the OS registration path.
    if let Some(launcher) = system.launcher {
        cfg.browsers.entry(system.kind).or_insert(launcher);
    }
    Ok(system.kind)
}

pub fn info(cfg: &Config) -> DefaultBrowserInfo {
    let system = system_browser("https");
    let system_kind = system.as_ref().ok().map(|entry| entry.kind);
    let system_error = system.as_ref().err().cloned();
    let mut snapshot = cfg.clone();
    let selection = select(&mut snapshot, || system);
    DefaultBrowserInfo {
        wsl: crate::wsl_browser::active(),
        preferred: cfg.default_browser,
        system: system_kind,
        system_error,
        effective: selection.as_ref().ok().copied(),
        error: selection.err(),
    }
}

pub async fn launch(
    mut cfg: Config,
    url: Option<String>,
) -> Result<(BrowserKind, Option<PathBuf>), String> {
    browser::validate_url(url.as_deref().unwrap_or("about:blank"))?;
    let scheme = if url.as_deref().is_some_and(|url| url.starts_with("http://")) {
        "http"
    } else {
        "https"
    };
    let (cfg, kind) = tokio::task::spawn_blocking(move || {
        let kind = select(&mut cfg, || system_browser(scheme))?;
        Ok::<_, String>((cfg, kind))
    })
    .await
    .map_err(|e| e.to_string())??;
    let profile = browser::launch(&cfg, kind, url).await?;
    Ok((kind, profile))
}

#[cfg(any(windows, target_os = "linux", test))]
fn windows_kind(path: &str) -> Result<BrowserKind, String> {
    match path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase()
        .as_str()
    {
        "chrome.exe" | "chromium.exe" => Ok(BrowserKind::Chrome),
        "msedge.exe" => Ok(BrowserKind::Edge),
        "firefox.exe" => Ok(BrowserKind::Firefox),
        _ => Err(unknown(path)),
    }
}

#[cfg(windows)]
fn system_browser(scheme: &str) -> Result<SystemBrowser, String> {
    #[link(name = "shlwapi")]
    extern "system" {
        fn AssocQueryStringW(
            flags: u32,
            value: u32,
            assoc: *const u16,
            extra: *const u16,
            output: *mut u16,
            length: *mut u32,
        ) -> i32;
    }
    let assoc: Vec<u16> = scheme.encode_utf16().chain(Some(0)).collect();
    let mut length = 0;
    // ASSOCF_IS_PROTOCOL queries the current user's protocol association;
    // ASSOCSTR_EXECUTABLE returns a path without shell arguments.
    unsafe {
        AssocQueryStringW(
            0x1000,
            2,
            assoc.as_ptr(),
            std::ptr::null(),
            std::ptr::null_mut(),
            &mut length,
        );
    }
    if length == 0 || length > 32768 {
        return Err(
            "OS 기본 브라우저를 확인할 수 없습니다. PK 웹에서 브라우저를 선택하세요.".into(),
        );
    }
    let mut output = vec![0u16; length as usize];
    let result = unsafe {
        AssocQueryStringW(
            0x1000,
            2,
            assoc.as_ptr(),
            std::ptr::null(),
            output.as_mut_ptr(),
            &mut length,
        )
    };
    if result != 0 {
        return Err(format!(
            "OS 기본 브라우저 조회 실패 ({result:#x}). PK 웹에서 브라우저를 선택하세요."
        ));
    }
    let end = output
        .iter()
        .position(|&character| character == 0)
        .unwrap_or(output.len());
    let path = String::from_utf16(&output[..end]).map_err(|e| e.to_string())?;
    Ok(SystemBrowser {
        kind: windows_kind(&path)?,
        launcher: Some(Launcher::Executable { path }),
    })
}

#[cfg(any(target_os = "macos", test))]
fn mac_kind(bundle: &str) -> Result<BrowserKind, String> {
    match bundle.to_ascii_lowercase().as_str() {
        "com.apple.safari" => Ok(BrowserKind::Safari),
        "com.google.chrome" | "org.chromium.chromium" => Ok(BrowserKind::Chrome),
        "com.microsoft.edgemac" => Ok(BrowserKind::Edge),
        "org.mozilla.firefox" => Ok(BrowserKind::Firefox),
        _ => Err(unknown(bundle)),
    }
}

#[cfg(target_os = "macos")]
fn system_browser(scheme: &str) -> Result<SystemBrowser, String> {
    use std::ffi::{c_char, c_void, CStr, CString};
    type CFRef = *const c_void;
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringCreateWithCString(
            allocator: CFRef,
            value: *const c_char,
            encoding: u32,
        ) -> CFRef;
        fn CFStringGetLength(value: CFRef) -> isize;
        fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
        fn CFStringGetCString(
            value: CFRef,
            output: *mut c_char,
            size: isize,
            encoding: u32,
        ) -> bool;
        fn CFURLCopyFileSystemPath(url: CFRef, style: isize) -> CFRef;
        fn CFRelease(value: CFRef);
    }
    #[link(name = "CoreServices", kind = "framework")]
    extern "C" {
        fn LSCopyDefaultHandlerForURLScheme(scheme: CFRef) -> CFRef;
        fn LSFindApplicationForInfo(
            creator: u32,
            bundle: CFRef,
            name: CFRef,
            fs_ref: *mut c_void,
            url: *mut CFRef,
        ) -> i32;
    }
    const UTF8: u32 = 0x08000100;
    struct Owned(CFRef);
    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe { CFRelease(self.0) };
        }
    }
    fn owned(value: CFRef) -> Result<Owned, String> {
        if value.is_null() {
            Err("OS 기본 브라우저를 확인할 수 없습니다. PK 웹에서 브라우저를 선택하세요.".into())
        } else {
            Ok(Owned(value))
        }
    }
    fn as_string(value: CFRef) -> Result<String, String> {
        let size = unsafe { CFStringGetMaximumSizeForEncoding(CFStringGetLength(value), UTF8) } + 1;
        if size <= 0 {
            return Err("OS 기본 브라우저 문자열을 읽을 수 없습니다.".into());
        }
        let mut output = vec![0u8; size as usize];
        if !unsafe { CFStringGetCString(value, output.as_mut_ptr().cast(), size, UTF8) } {
            return Err("OS 기본 브라우저 문자열을 읽을 수 없습니다.".into());
        }
        Ok(unsafe { CStr::from_ptr(output.as_ptr().cast()) }
            .to_string_lossy()
            .into_owned())
    }
    let scheme = CString::new(scheme).map_err(|e| e.to_string())?;
    let scheme =
        owned(unsafe { CFStringCreateWithCString(std::ptr::null(), scheme.as_ptr(), UTF8) })?;
    let bundle = owned(unsafe { LSCopyDefaultHandlerForURLScheme(scheme.0) })?;
    let identity = as_string(bundle.0)?;
    let kind = mac_kind(&identity)?;
    let mut app_url = std::ptr::null();
    if unsafe {
        LSFindApplicationForInfo(
            0,
            bundle.0,
            std::ptr::null(),
            std::ptr::null_mut(),
            &mut app_url,
        )
    } != 0
    {
        return Err(
            "OS 기본 브라우저 앱 위치를 확인할 수 없습니다. PK 웹에서 실행 위치를 지정하세요."
                .into(),
        );
    }
    let app_url = owned(app_url)?;
    let app_path = owned(unsafe { CFURLCopyFileSystemPath(app_url.0, 0) })?;
    let executable = match identity.as_str() {
        "org.chromium.Chromium" => "Chromium",
        _ => match kind {
            BrowserKind::Chrome => "Google Chrome",
            BrowserKind::Edge => "Microsoft Edge",
            BrowserKind::Firefox => "firefox",
            BrowserKind::Safari => "Safari",
        },
    };
    let path = PathBuf::from(as_string(app_path.0)?)
        .join("Contents/MacOS")
        .join(executable);
    Ok(SystemBrowser {
        kind,
        launcher: Some(Launcher::Executable {
            path: path.to_string_lossy().into_owned(),
        }),
    })
}

#[cfg(any(target_os = "linux", test))]
fn linux_browser(desktop: &str) -> Result<SystemBrowser, String> {
    let kind = match desktop.to_ascii_lowercase().as_str() {
        "google-chrome.desktop"
        | "google-chrome-stable.desktop"
        | "chromium.desktop"
        | "chromium-browser.desktop"
        | "chromium_chromium.desktop"
        | "com.google.chrome.desktop"
        | "org.chromium.chromium.desktop" => BrowserKind::Chrome,
        "microsoft-edge.desktop"
        | "microsoft-edge-stable.desktop"
        | "com.microsoft.edge.desktop" => BrowserKind::Edge,
        "firefox.desktop"
        | "firefox-esr.desktop"
        | "firefox_firefox.desktop"
        | "org.mozilla.firefox.desktop" => BrowserKind::Firefox,
        _ => return Err(unknown(desktop)),
    };
    let launcher = match desktop {
        "com.google.Chrome.desktop" => Some(Launcher::Flatpak {
            app_id: "com.google.Chrome".into(),
        }),
        "org.chromium.Chromium.desktop" => Some(Launcher::Flatpak {
            app_id: "org.chromium.Chromium".into(),
        }),
        "com.microsoft.Edge.desktop" => Some(Launcher::Flatpak {
            app_id: "com.microsoft.Edge".into(),
        }),
        "org.mozilla.firefox.desktop" => Some(Launcher::Flatpak {
            app_id: "org.mozilla.firefox".into(),
        }),
        "firefox_firefox.desktop" => Some(Launcher::Snap {
            name: "firefox".into(),
        }),
        "chromium_chromium.desktop" => Some(Launcher::Snap {
            name: "chromium".into(),
        }),
        "chromium.desktop" => Some(Launcher::Executable {
            path: "chromium".into(),
        }),
        "chromium-browser.desktop" => Some(Launcher::Executable {
            path: "chromium-browser".into(),
        }),
        "firefox-esr.desktop" => Some(Launcher::Executable {
            path: "firefox-esr".into(),
        }),
        _ => None,
    };
    Ok(SystemBrowser { kind, launcher })
}

#[cfg(any(target_os = "linux", test))]
fn desktop_executable(content: &str) -> Option<String> {
    let mut entry = false;
    for line in content.lines().map(str::trim) {
        if line.starts_with('[') {
            entry = line == "[Desktop Entry]";
        }
        if !entry {
            continue;
        }
        if let Some(command) = line.strip_prefix("Exec=") {
            // Decode only the executable token. Desktop options and URL field codes
            // are replaced by PK's proxy/profile arguments; no shell is invoked.
            let mut token = String::new();
            let mut quoted = false;
            let mut escaped = false;
            for character in command.trim().chars() {
                if escaped {
                    token.push(character);
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == '"' {
                    quoted = !quoted;
                } else if character.is_whitespace() && !quoted {
                    break;
                } else {
                    token.push(character);
                }
            }
            if quoted
                || escaped
                || token.is_empty()
                || token.contains('%')
                || token.starts_with('-')
            {
                return None;
            }
            return Some(token);
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn linux_registered_browser(desktop: &str) -> Result<SystemBrowser, String> {
    use std::{env, fs};
    let mut browser = linux_browser(desktop)?;
    let mut directories = Vec::new();
    if let Some(home) = env::var_os("HOME") {
        let home = PathBuf::from(home);
        directories.push(
            env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".local/share")),
        );
        directories.push(home.join(".local/share/flatpak/exports/share"));
    }
    let data_dirs =
        env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    directories.extend(env::split_paths(&data_dirs));
    directories.push(PathBuf::from("/var/lib/flatpak/exports/share"));
    directories.push(PathBuf::from("/var/lib/snapd/desktop"));
    for directory in directories {
        if let Ok(content) = fs::read_to_string(directory.join("applications").join(desktop)) {
            if let Some(path) = desktop_executable(&content) {
                let name = PathBuf::from(&path)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if !matches!(name.as_str(), "env" | "flatpak" | "snap") {
                    browser.launcher = Some(Launcher::Executable { path });
                }
            }
            break;
        }
    }
    Ok(browser)
}

#[cfg(target_os = "linux")]
fn system_browser(scheme: &str) -> Result<SystemBrowser, String> {
    use std::process::{Command, Stdio};
    if crate::wsl_browser::active() {
        let path = crate::wsl_browser::default_executable(scheme)?;
        let kind = windows_kind(&path)?;
        let path = crate::wsl_browser::input_path(&path)?;
        return Ok(SystemBrowser { kind, launcher: Some(Launcher::Executable { path: path.to_string_lossy().into_owned() }) });
    }
    // Protocol associations take priority; the desktop's generic default is a fallback.
    let mime = format!("x-scheme-handler/{scheme}");
    for (program, args) in [
        ("xdg-mime", vec!["query", "default", mime.as_str()]),
        ("xdg-settings", vec!["get", "default-web-browser"]),
    ] {
        if let Ok(output) = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
        {
            let identity = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if output.status.success() && !identity.is_empty() {
                return linux_registered_browser(&identity);
            }
        }
    }
    Err("OS 기본 브라우저를 확인할 수 없습니다. PK 웹에서 브라우저를 선택하세요.".into())
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn system_browser(_: &str) -> Result<SystemBrowser, String> {
    Err("이 OS의 기본 브라우저 조회를 지원하지 않습니다. PK 웹에서 브라우저를 선택하세요.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_registered_browsers_without_guessing_unknown_apps() {
        assert_eq!(
            windows_kind(r"D:\앱 폴더\MSedge.exe").unwrap(),
            BrowserKind::Edge
        );
        assert!(windows_kind("not-firefox.exe").is_err());
        assert_eq!(mac_kind("com.apple.Safari").unwrap(), BrowserKind::Safari);
        assert!(mac_kind("com.brave.Browser").is_err());
        assert_eq!(
            linux_browser("firefox-esr.desktop").unwrap().kind,
            BrowserKind::Firefox
        );
        assert!(matches!(
            linux_browser("org.mozilla.firefox.desktop")
                .unwrap()
                .launcher,
            Some(Launcher::Flatpak { .. })
        ));
        assert!(matches!(
            linux_browser("firefox_firefox.desktop").unwrap().launcher,
            Some(Launcher::Snap { .. })
        ));
        assert!(linux_browser("custom-firefox.desktop").is_err());
    }

    #[test]
    fn desktop_entry_preserves_custom_executable_and_ignores_action_commands() {
        assert_eq!(desktop_executable("[Desktop Entry]\nExec=\"/opt/브라우저 폴더/firefox\" %u\n[Desktop Action NewWindow]\nExec=wrong"), Some("/opt/브라우저 폴더/firefox".into()));
        assert_eq!(
            desktop_executable(
                "[Desktop Action NewWindow]\nExec=wrong\n[Desktop Entry]\nExec=firefox %u"
            ),
            Some("firefox".into())
        );
        assert_eq!(
            desktop_executable("[Desktop Entry]\nExec=\"/unclosed path"),
            None
        );
        assert_eq!(desktop_executable("[Desktop Entry]\nExec=%u"), None);
    }

    #[test]
    fn web_preference_overrides_os_and_preserves_saved_paths() {
        let mut cfg = Config {
            default_browser: Some(BrowserKind::Edge),
            ..Config::default()
        };
        assert_eq!(
            select(&mut cfg, || panic!(
                "OS lookup unnecessary for a saved preference"
            ))
            .unwrap(),
            BrowserKind::Edge
        );
        cfg.default_browser = None;
        let saved = Launcher::Executable {
            path: "manual.exe".into(),
        };
        cfg.browsers.insert(BrowserKind::Chrome, saved.clone());
        assert_eq!(
            select(&mut cfg, || Ok(SystemBrowser {
                kind: BrowserKind::Chrome,
                launcher: Some(Launcher::Executable {
                    path: "os.exe".into()
                })
            }))
            .unwrap(),
            BrowserKind::Chrome
        );
        assert_eq!(cfg.browsers.get(&BrowserKind::Chrome), Some(&saved));
        assert!(select(&mut cfg, || Err(unknown("Brave"))).is_err());
    }

    #[test]
    fn old_config_follows_os_and_preference_roundtrips() {
        let mut cfg: Config = toml::from_str("socks_port = 1080").unwrap();
        assert_eq!(cfg.default_browser, None);
        cfg.default_browser = Some(BrowserKind::Firefox);
        let saved = toml::to_string(&cfg).unwrap();
        assert_eq!(
            toml::from_str::<Config>(&saved).unwrap().default_browser,
            Some(BrowserKind::Firefox)
        );
    }
}
