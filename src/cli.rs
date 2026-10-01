use crate::config::Config;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn install_symlinks() -> Result<(), String> {
    let bin_dir = get_install_bin_dir()?;
    fs::create_dir_all(&bin_dir).map_err(|e| format!("Failed to create bin dir: {}", e))?;

    let current_exe = env::current_exe().map_err(|e| format!("Failed to get exe path: {}", e))?;

    let commands = [
        ("pk", ""),
        ("codex-app-proxy", "desktop"),
        ("chatgpt-pk", "desktop"),
        ("codex-proxy", "codex"),
        ("grok-proxy", "grok"),
        ("claude-proxy", "claude"),
        ("agy-proxy", "agy"),
        ("ocx-proxy", "ocx"),
        ("opencodex-proxy", "ocx"),
    ];

    println!("==> {} 경로에 명령어를 등록합니다...", bin_dir.display());

    for (cmd_name, _target) in &commands {
        #[cfg(unix)]
        {
            let link_target = bin_dir.join(cmd_name);
            if link_target.exists() || fs::symlink_metadata(&link_target).is_ok() {
                let _ = fs::remove_file(&link_target);
            }
            std::os::unix::fs::symlink(&current_exe, &link_target)
                .map_err(|e| format!("Failed to symlink {}: {}", cmd_name, e))?;
            println!("  [등록됨] {} -> {}", cmd_name, current_exe.display());
        }

        #[cfg(windows)]
        {
            let cmd_file = bin_dir.join(format!("{}.cmd", cmd_name));
            let content = if _target.is_empty() {
                format!(r#"@"{}" %*
"#, current_exe.display())
            } else {
                format!(r#"@"{}" {} %*
"#, current_exe.display(), _target)
            };
            fs::write(&cmd_file, content)
                .map_err(|e| format!("Failed to write shim {}: {}", cmd_file.display(), e))?;
            println!("  [등록됨] {} -> {}", cmd_file.display(), current_exe.display());
        }
    }

    #[cfg(windows)]
    update_windows_user_path(&bin_dir, true)?;

    println!("
✅ 설치가 완료되었습니다!");
    println!("새 터미널에서 'pk', 'pk stop', 'codex-proxy' 등을 실행할 수 있습니다.");
    Ok(())
}

fn get_install_bin_dir() -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        if let Ok(localappdata) = env::var("LOCALAPPDATA") {
            return Ok(PathBuf::from(localappdata).join("pk").join("bin"));
        }
        if let Ok(userprofile) = env::var("USERPROFILE") {
            return Ok(PathBuf::from(userprofile).join("AppData").join("Local").join("pk").join("bin"));
        }
    }

    if let Ok(home) = env::var("HOME") {
        return Ok(PathBuf::from(home).join(".local/bin"));
    }

    Err("Could not determine user bin directory".to_string())
}

#[cfg(windows)]
fn update_windows_user_path(bin_dir: &Path, install: bool) -> Result<(), String> {
    // Pass the directory as data so spaces and quotes cannot become PowerShell code.
    let script = r#"
$ErrorActionPreference = 'Stop'
$dir = $env:PK_INSTALL_BIN_DIR
$items = @([Environment]::GetEnvironmentVariable('Path', 'User') -split ';' | Where-Object { $_ })
$normal = $dir.TrimEnd('\')
if ($env:PK_INSTALL_PATH_ACTION -eq 'add') {
    if (-not @($items | Where-Object { $_.TrimEnd('\') -ieq $normal }).Count) {
        [Environment]::SetEnvironmentVariable('Path', (($items + $dir) -join ';'), 'User')
    }
} else {
    $kept = @($items | Where-Object { $_.TrimEnd('\') -ine $normal })
    if ($kept.Count -ne $items.Count) {
        [Environment]::SetEnvironmentVariable('Path', ($kept -join ';'), 'User')
    }
}
"#;
    let status = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("PK_INSTALL_BIN_DIR", bin_dir)
        .env("PK_INSTALL_PATH_ACTION", if install { "add" } else { "remove" })
        .status()
        .map_err(|e| format!("Failed to update user PATH: {}", e))?;
    if !status.success() {
        return Err(format!("Failed to update user PATH (exit code: {})", status));
    }
    Ok(())
}

#[cfg(windows)]
fn resolve_windows_program(target: &str, search_path: &std::ffi::OsStr) -> Option<PathBuf> {
    // Rust only adds .exe when searching PATH. npm CLIs use .cmd shims.
    // Search each directory in order so the installed npm CLI takes precedence
    // over a different copy of the executable later in PATH.
    for directory in env::split_paths(search_path) {
        for extension in ["exe", "com", "cmd", "bat"] {
            let candidate = directory.join(format!("{}.{}", target, extension));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

pub fn run_proxied_command(target_binary: &str, args: &[String]) -> ! {
    let config = Config::load();
    let http_url = format!("http://127.0.0.1:{}", config.http_port);

    #[cfg(windows)]
    let program = env::var_os("PATH")
        .and_then(|search_path| resolve_windows_program(target_binary, &search_path))
        .unwrap_or_else(|| PathBuf::from(target_binary));
    #[cfg(unix)]
    let program = target_binary;
    let mut cmd = Command::new(program);
    cmd.args(args);

    cmd.env("HTTP_PROXY", &http_url)
        .env("HTTPS_PROXY", &http_url)
        .env("WS_PROXY", &http_url)
        .env("WSS_PROXY", &http_url)
        .env("http_proxy", &http_url)
        .env("https_proxy", &http_url)
        .env("ws_proxy", &http_url)
        .env("wss_proxy", &http_url)
        .env("NO_PROXY", &config.no_proxy)
        .env("no_proxy", &config.no_proxy);

    if target_binary == "agy" {
        if let Ok(home) = env::var("HOME") {
            let pw_path = format!("{}/.cache/ms-playwright-go/1.57.0", home);
            if Path::new(&format!("{}/package", pw_path)).exists() {
                cmd.env("PLAYWRIGHT_DRIVER_PATH", pw_path);
            }
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = cmd.exec();
        eprintln!("Failed to execute '{}': {}", target_binary, err);
        std::process::exit(127);
    }

    #[cfg(windows)]
    {
        match cmd.status() {
            Ok(status) => {
                std::process::exit(status.code().unwrap_or(0));
            }
            Err(err) => {
                eprintln!("Failed to execute '{}': {}", target_binary, err);
                std::process::exit(127);
            }
        }
    }
}

pub fn uninstall_symlinks() -> Result<(), String> {
    let bin_dir = get_install_bin_dir()?;
    let commands = [
        "pk",
        "codex-proxy",
        "codex-app-proxy",
        "chatgpt-pk",
        "grok-proxy",
        "claude-proxy",
        "agy-proxy",
        "ocx-proxy",
        "opencodex-proxy",
    ];

    println!("==> {} 경로에서 설치된 명령어를 제거합니다...", bin_dir.display());

    for cmd_name in &commands {
        #[cfg(unix)]
        {
            let link_target = bin_dir.join(cmd_name);
            if link_target.exists() || fs::symlink_metadata(&link_target).is_ok() {
                let _ = fs::remove_file(&link_target);
                println!("  [제거됨] {}", cmd_name);
            }
        }

        #[cfg(windows)]
        {
            let cmd_file = bin_dir.join(format!("{}.cmd", cmd_name));
            if cmd_file.exists() {
                let _ = fs::remove_file(&cmd_file);
                println!("  [제거됨] {}", cmd_file.display());
            }
        }
    }

    #[cfg(windows)]
    update_windows_user_path(&bin_dir, false)?;

    println!("
✅ 삭제가 완료되었습니다!");
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn runs_npm_cmd_shim_with_spaces_proxy_environment_and_exit_code() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = env::temp_dir()
            .join(format!("pk npm shim {} {}", std::process::id(), unique));
        fs::create_dir_all(&directory).unwrap();
        let shim = directory.join("codex.cmd");
        fs::write(
            &shim,
            "@echo off\r\necho %HTTP_PROXY%\r\necho %~1\r\necho %~2\r\nexit /b 23\r\n",
        ).unwrap();
        let later = directory.join("later");
        fs::create_dir(&later).unwrap();
        fs::write(later.join("codex.exe"), b"different CLI later in PATH").unwrap();
        let search_path = env::join_paths([&directory, &later]).unwrap();
        let program = resolve_windows_program("codex", &search_path).unwrap();
        assert_eq!(program, shim);
        let result = Command::new(program)
            .args(["hello world", "--version"])
            .env("HTTP_PROXY", "http://127.0.0.1:3128")
            .output()
            .unwrap();
        fs::remove_file(shim).unwrap();
        fs::remove_file(later.join("codex.exe")).unwrap();
        fs::remove_dir(later).unwrap();
        fs::remove_dir(directory).unwrap();
        assert_eq!(result.status.code(), Some(23));
        assert_eq!(
            String::from_utf8(result.stdout).unwrap().lines().collect::<Vec<_>>(),
            ["http://127.0.0.1:3128", "hello world", "--version"],
        );
    }
}
