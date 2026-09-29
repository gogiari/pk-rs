#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod windows {
    use std::env;
    use std::ffi::{c_void, OsStr};
    use std::fs;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const MB_ICONERROR: u32 = 0x10;
    const MB_ICONINFORMATION: u32 = 0x40;
    const MB_YESNO: u32 = 0x04;
    const IDYES: i32 = 6;
    const UNINSTALLER_NAME: &str = "pk-uninstaller.exe";
    const PK_EXE: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/target/release/pk.exe"
    ));

    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(hwnd: *mut c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }

    pub fn main() {
        if env::args().any(|arg| arg == "--uninstall") {
            uninstall_main();
            return;
        }
        match install() {
            Ok(installed_exe) => {
                let message = format!(
                    "PK Proxy Manager 설치가 완료되었습니다.\n\n설치 위치: {}\n시작 메뉴와 바탕화면에서 실행할 수 있습니다.\n새 터미널에서는 pk 명령을 사용할 수 있습니다.\n\n지금 대시보드를 열까요?",
                    installed_exe.display()
                );
                if message_box(&message, MB_YESNO | MB_ICONINFORMATION) == IDYES {
                    if let Err(error) = run_cli(&installed_exe, &["ui"]) {
                        message_box(
                            &format!("대시보드를 열지 못했습니다.\n\n{error}"),
                            MB_ICONERROR,
                        );
                    }
                }
            }
            Err(error) => {
                message_box(&format!("설치하지 못했습니다.\n\n{error}"), MB_ICONERROR);
                std::process::exit(1);
            }
        }
    }

    fn install() -> Result<PathBuf, String> {
        let local_app_data = env::var_os("LOCALAPPDATA")
            .ok_or_else(|| "LOCALAPPDATA 경로를 찾을 수 없습니다.".to_string())?;
        let bin_dir = PathBuf::from(local_app_data).join("pk").join("bin");
        fs::create_dir_all(&bin_dir).map_err(|e| format!("설치 폴더 생성 실패: {e}"))?;
        let installed_exe = bin_dir.join("pk.exe");

        let is_current = fs::read(&installed_exe)
            .map(|bytes| bytes == PK_EXE)
            .unwrap_or(false);
        if !is_current {
            if installed_exe.exists() {
                run_cli(&installed_exe, &["stop"])?;
            }
            replace_exe(&bin_dir, &installed_exe)?;
        }

        run_cli(&installed_exe, &["install"])?;
        let uninstaller = bin_dir.join(UNINSTALLER_NAME);
        let current_exe = env::current_exe().map_err(|e| format!("설치 파일 경로 확인 실패: {e}"))?;
        if current_exe != uninstaller {
            fs::copy(&current_exe, &uninstaller)
                .map_err(|e| format!("제거 프로그램 설치 실패: {e}"))?;
        }
        create_shortcuts_and_registration(&installed_exe, &uninstaller)?;
        Ok(installed_exe)
    }

    fn uninstall_main() {
        if message_box(
            "PK Proxy Manager와 바로가기, 전역 명령을 제거할까요?\n개인 설정은 보존됩니다.",
            MB_YESNO | MB_ICONINFORMATION,
        ) != IDYES {
            return;
        }
        match uninstall() {
            Ok(()) => {
                message_box("PK Proxy Manager가 제거되었습니다. 개인 설정은 보존됩니다.", MB_ICONINFORMATION);
            }
            Err(error) => {
                message_box(&format!("제거하지 못했습니다.\n\n{error}"), MB_ICONERROR);
                std::process::exit(1);
            }
        }
    }

    fn uninstall() -> Result<(), String> {
        let local_app_data = env::var_os("LOCALAPPDATA")
            .ok_or_else(|| "LOCALAPPDATA 경로를 찾을 수 없습니다.".to_string())?;
        let bin_dir = PathBuf::from(local_app_data).join("pk").join("bin");
        let installed_exe = bin_dir.join("pk.exe");
        let uninstaller = bin_dir.join(UNINSTALLER_NAME);
        if !installed_exe.is_file() || !uninstaller.is_file() {
            return Err("설치된 PK Proxy Manager를 찾을 수 없습니다.".to_string());
        }

        run_cli(&installed_exe, &["stop"])?;
        run_cli(&installed_exe, &["uninstall"])?;
        remove_shortcuts_and_registration()?;
        fs::remove_file(&installed_exe).map_err(|e| format!("프로그램 파일 제거 실패: {e}"))?;
        schedule_uninstaller_cleanup(&uninstaller, &bin_dir)?;
        Ok(())
    }

    fn replace_exe(bin_dir: &Path, installed_exe: &Path) -> Result<(), String> {
        let staged = bin_dir.join("pk.new.exe");
        let backup = bin_dir.join("pk.previous.exe");
        fs::write(&staged, PK_EXE).map_err(|e| format!("실행 파일 준비 실패: {e}"))?;

        if installed_exe.exists() {
            if backup.exists() {
                fs::remove_file(&backup).map_err(|e| format!("이전 백업 정리 실패: {e}"))?;
            }
            if let Err(error) = fs::rename(installed_exe, &backup) {
                let _ = fs::remove_file(&staged);
                return Err(format!("기존 실행 파일 교체 실패: {error}"));
            }
        }

        if let Err(error) = fs::rename(&staged, installed_exe) {
            if backup.exists() {
                let _ = fs::rename(&backup, installed_exe);
            }
            return Err(format!("새 실행 파일 설치 실패: {error}"));
        }
        if backup.exists() {
            let _ = fs::remove_file(&backup);
        }
        Ok(())
    }

    fn run_cli(exe: &Path, args: &[&str]) -> Result<(), String> {
        let output = Command::new(exe)
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("{} 실행 실패: {e}", exe.display()))?;
        if output.status.success() {
            return Ok(());
        }
        let detail = String::from_utf8_lossy(&output.stderr);
        let detail = if detail.trim().is_empty() {
            String::from_utf8_lossy(&output.stdout).into_owned()
        } else {
            detail.into_owned()
        };
        Err(format!(
            "{} {} 실패: {}",
            exe.display(),
            args.join(" "),
            detail.trim()
        ))
    }

    fn create_shortcuts_and_registration(installed_exe: &Path, uninstaller: &Path) -> Result<(), String> {
        let script = r#"
$ErrorActionPreference = 'Stop'
$target = $env:PK_INSTALLED_EXE
$uninstaller = $env:PK_UNINSTALLER_EXE
$shell = New-Object -ComObject WScript.Shell
foreach ($location in @((Join-Path ([Environment]::GetFolderPath('Programs')) 'PK Proxy Manager.lnk'), (Join-Path ([Environment]::GetFolderPath('DesktopDirectory')) 'PK Proxy Manager.lnk'))) {
    $shortcut = $shell.CreateShortcut($location)
    $shortcut.TargetPath = $target
    $shortcut.WorkingDirectory = Split-Path -Parent $target
    $shortcut.IconLocation = "$target,0"
    $shortcut.Save()
}
$shortcut = $shell.CreateShortcut((Join-Path ([Environment]::GetFolderPath('Programs')) 'Uninstall PK Proxy Manager.lnk'))
$shortcut.TargetPath = $uninstaller
$shortcut.Arguments = '--uninstall'
$shortcut.WorkingDirectory = Split-Path -Parent $uninstaller
$shortcut.Save()
$key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Software\Microsoft\Windows\CurrentVersion\Uninstall\PKProxyManager')
try {
    $key.SetValue('DisplayName', 'PK Proxy Manager')
    $key.SetValue('DisplayVersion', $env:PK_VERSION)
    $key.SetValue('Publisher', 'PK Proxy Manager')
    $key.SetValue('InstallLocation', (Split-Path -Parent $target))
    $key.SetValue('DisplayIcon', $target)
    $key.SetValue('UninstallString', ('"' + $uninstaller + '" --uninstall'))
    $key.SetValue('NoModify', 1, [Microsoft.Win32.RegistryValueKind]::DWord)
    $key.SetValue('NoRepair', 1, [Microsoft.Win32.RegistryValueKind]::DWord)
} finally { $key.Close() }
"#;
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("PK_INSTALLED_EXE", installed_exe)
            .env("PK_UNINSTALLER_EXE", uninstaller)
            .env("PK_VERSION", env!("CARGO_PKG_VERSION"))
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("바로가기 생성 실패: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "바로가기 생성 실패: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(())
    }

    fn remove_shortcuts_and_registration() -> Result<(), String> {
        let script = r#"
$ErrorActionPreference = 'Stop'
foreach ($location in @(
    (Join-Path ([Environment]::GetFolderPath('Programs')) 'PK Proxy Manager.lnk'),
    (Join-Path ([Environment]::GetFolderPath('DesktopDirectory')) 'PK Proxy Manager.lnk'),
    (Join-Path ([Environment]::GetFolderPath('Programs')) 'Uninstall PK Proxy Manager.lnk')
)) { if (Test-Path -LiteralPath $location) { Remove-Item -LiteralPath $location -Force } }
$key = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\PKProxyManager'
[Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($key, $false)
"#;
        run_powershell(script)
    }

    fn schedule_uninstaller_cleanup(uninstaller: &Path, bin_dir: &Path) -> Result<(), String> {
        let script = r#"
$ErrorActionPreference = 'Stop'
$parentPid = [int]$env:PK_UNINSTALLER_PID
for ($i = 0; $i -lt 120 -and (Get-Process -Id $parentPid -ErrorAction SilentlyContinue); $i++) {
    Start-Sleep -Milliseconds 250
}
Remove-Item -LiteralPath $env:PK_UNINSTALLER_EXE -Force
if ((Test-Path -LiteralPath $env:PK_INSTALL_BIN_DIR) -and
    -not (Get-ChildItem -LiteralPath $env:PK_INSTALL_BIN_DIR -Force | Select-Object -First 1)) {
    Remove-Item -LiteralPath $env:PK_INSTALL_BIN_DIR
}
"#;
        Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("PK_UNINSTALLER_PID", std::process::id().to_string())
            .env("PK_UNINSTALLER_EXE", uninstaller)
            .env("PK_INSTALL_BIN_DIR", bin_dir)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("제거 프로그램 정리 예약 실패: {e}"))?;
        Ok(())
    }

    fn run_powershell(script: &str) -> Result<(), String> {
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("시스템 등록 제거 실패: {e}"))?;
        if output.status.success() { Ok(()) } else {
            Err(format!("시스템 등록 제거 실패: {}", String::from_utf8_lossy(&output.stderr).trim()))
        }
    }

    fn message_box(message: &str, kind: u32) -> i32 {
        let text = wide(message);
        let caption = wide("PK Proxy Manager 설치");
        unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), kind) }
    }

    fn wide(text: &str) -> Vec<u16> {
        OsStr::new(text)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }
}

#[cfg(windows)]
fn main() {
    windows::main();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("pk-installer is available on Windows only.");
}
