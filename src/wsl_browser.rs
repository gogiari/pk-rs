//! WSL launches Windows browser executables directly through interop.
//! PowerShell only reads Windows settings and probes the forwarded proxy.
use crate::browser::{BrowserKind, Launcher};
use serde_json::{json, Value};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::OnceLock,
};

pub fn active() -> bool {
    static ACTIVE: OnceLock<bool> = OnceLock::new();
    *ACTIVE.get_or_init(|| {
        cfg!(target_os = "linux")
            && kernel_is_wsl(&fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default())
    })
}

fn kernel_is_wsl(kernel: &str) -> bool {
    let kernel = kernel.to_ascii_lowercase();
    kernel.contains("microsoft") || kernel.contains("wsl")
}

fn drive_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
}

fn translate(path: &Path, mode: &str) -> Result<PathBuf, String> {
    let output = Command::new("wslpath")
        .args(["-a", mode])
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("WSL 경로 변환 실패: {error}"))?;
    let value = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
    let value = value.trim_end_matches(['\r', '\n']);
    if !output.status.success() || value.is_empty() {
        return Err(format!("WSL 경로를 변환할 수 없습니다: {}", path.display()));
    }
    Ok(PathBuf::from(value))
}

pub fn input_path(path: &str) -> Result<PathBuf, String> {
    if active() && drive_path(path) {
        translate(Path::new(path), "-u")
    } else {
        Ok(PathBuf::from(path))
    }
}

pub fn windows_launcher(launcher: &Launcher) -> bool {
    if !active() {
        return false;
    }
    if let Launcher::Executable { path } = launcher {
        Path::new(path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    } else {
        false
    }
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        output.push(ALPHABET[(a >> 2) as usize] as char);
        output.push(ALPHABET[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        output.push(if chunk.len() > 1 {
            ALPHABET[(((b & 15) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            ALPHABET[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    output
}

fn powershell_json(script: &str, request: Value) -> Result<Value, String> {
    // User paths stay data, including quotes, dollars, backticks, and Unicode.
    let data = base64(&serde_json::to_vec(&request).map_err(|error| error.to_string())?);
    let script = format!("$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); $request=ConvertFrom-Json ([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{data}')));\n{script}");
    let mut command = Command::new("powershell.exe");
    command
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .stdin(Stdio::null());
    let output = match command.output() {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let path = input_path(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe")?;
            Command::new(path).args(["-NoProfile", "-NonInteractive", "-Command", &script]).stdin(Stdio::null()).output()
                .map_err(|error| format!("Windows PowerShell을 실행하지 못했습니다. WSL의 Windows interop 설정을 확인하세요: {error}"))?
        }
        Err(error) => return Err(format!("WSL에서 Windows를 조회하지 못했습니다: {error}")),
    };
    if !output.status.success() {
        return Err(format!(
            "Windows 설정 조회 실패: {}",
            String::from_utf8_lossy(&output.stderr)
                .trim()
                .chars()
                .take(600)
                .collect::<String>()
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Windows 응답을 읽지 못했습니다: {error}"))
}

const DETECT_SCRIPT: &str = r#"
$paths = [Collections.Generic.List[string]]::new()
foreach ($hive in @([Microsoft.Win32.RegistryHive]::CurrentUser, [Microsoft.Win32.RegistryHive]::LocalMachine)) {
    foreach ($view in @([Microsoft.Win32.RegistryView]::Registry64, [Microsoft.Win32.RegistryView]::Registry32)) {
        $base = [Microsoft.Win32.RegistryKey]::OpenBaseKey($hive, $view)
        try {
            $key = $base.OpenSubKey('Software\Microsoft\Windows\CurrentVersion\App Paths\' + $request.filename)
            if ($null -ne $key) {
                try { $value=$key.GetValue(''); if ($value) { $paths.Add(([string]$value).Trim('"')) } }
                finally { $key.Dispose() }
            }
        } finally { $base.Dispose() }
    }
}
foreach ($root in @($env:LOCALAPPDATA, $env:ProgramFiles, ${env:ProgramFiles(x86)})) {
    if ($root) { $paths.Add((Join-Path $root $request.relative)) }
}
ConvertTo-Json -InputObject @($paths.ToArray()) -Compress
"#;

pub fn detect(kind: BrowserKind) -> Option<Launcher> {
    let (filename, relative) = match kind {
        BrowserKind::Chrome => ("chrome.exe", r"Google\Chrome\Application\chrome.exe"),
        BrowserKind::Edge => ("msedge.exe", r"Microsoft\Edge\Application\msedge.exe"),
        BrowserKind::Firefox => ("firefox.exe", r"Mozilla Firefox\firefox.exe"),
        BrowserKind::Safari => return None,
    };
    let paths: Vec<String> = serde_json::from_value(
        powershell_json(
            DETECT_SCRIPT,
            json!({"filename": filename, "relative": relative}),
        )
        .ok()?,
    )
    .ok()?;
    paths
        .into_iter()
        .filter_map(|path| input_path(&path).ok())
        .find(|path| path.is_file())
        .map(|path| Launcher::Executable {
            path: path.to_string_lossy().into_owned(),
        })
}

#[cfg(target_os = "linux")]
const DEFAULT_SCRIPT: &str = r#"
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class PKBrowserAssociation {
    [DllImport("Shlwapi.dll", EntryPoint="AssocQueryStringW", CharSet=CharSet.Unicode, ExactSpelling=true)]
    private static extern int AssocQueryString(uint flags, uint value, string assoc, string extra, StringBuilder output, ref uint length);
    public static string Resolve(string scheme) {
        uint length=0;
        AssocQueryString(0x1000,2,scheme,null,null,ref length);
        if (length==0 || length>32768) throw new Exception("Windows default browser association unavailable");
        StringBuilder buffer=new StringBuilder((int)length);
        int result=AssocQueryString(0x1000,2,scheme,null,buffer,ref length);
        if (result!=0) throw new Exception("Windows browser query failed: " + result);
        return buffer.ToString();
    }
}
'@
ConvertTo-Json -InputObject ([PKBrowserAssociation]::Resolve([string]$request.scheme)) -Compress
"#;

#[cfg(target_os = "linux")]
pub fn default_executable(scheme: &str) -> Result<String, String> {
    serde_json::from_value(powershell_json(DEFAULT_SCRIPT, json!({"scheme": scheme}))?)
        .map_err(|error| error.to_string())
}

pub fn argument_profile(profile: &Path) -> Result<PathBuf, String> {
    let path = translate(profile, "-w")?;
    if !drive_path(&path.to_string_lossy()) {
        return Err("Windows 브라우저 프로필은 Windows 드라이브에 저장해야 합니다.".into());
    }
    Ok(path)
}

pub fn profile_path(kind: BrowserKind, linux_profile: &Path) -> Result<PathBuf, String> {
    // A config folder already on a Windows drive can share its normal profile layout.
    let full = if linux_profile.is_absolute() {
        linux_profile.to_owned()
    } else {
        env::current_dir()
            .map_err(|error| error.to_string())?
            .join(linux_profile)
    };
    if argument_profile(&full).is_ok() {
        return Ok(full);
    }
    let local: String = serde_json::from_value(powershell_json(
        "ConvertTo-Json -InputObject $env:LOCALAPPDATA -Compress",
        json!({}),
    )?)
    .map_err(|error| error.to_string())?;
    if !drive_path(&local) {
        return Err("Windows LOCALAPPDATA 경로를 확인할 수 없습니다.".into());
    }
    let distro = env::var("WSL_DISTRO_NAME").unwrap_or_else(|_| "wsl".into());
    let distro: String = distro
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || "-_".contains(character) {
                character
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    // Independent PK configurations must not race over the same Firefox user.js.
    let key = full
        .to_string_lossy()
        .bytes()
        .fold(0xcbf29ce484222325u64, |hash, byte| {
            (hash ^ byte as u64).wrapping_mul(0x100000001b3)
        });
    input_path(&format!(
        r"{local}\pk\wsl-browser-profiles\{distro}\{key:016x}\{}",
        kind.id()
    ))
}

const PROBE_SCRIPT: &str = r#"
$client=[Net.Sockets.TcpClient]::new()
try {
    $connect=$client.ConnectAsync('127.0.0.1',[int]$request.port)
    if (!$connect.Wait(2000)) { throw 'Windows localhost 연결 시간 초과' }
    $stream=$client.GetStream(); $stream.ReadTimeout=2000; $stream.WriteTimeout=2000
    $stream.Write([byte[]](5,1,0),0,3)
    $reply=[byte[]]::new(2); $read=0
    while ($read -lt 2) {
        $count=$stream.Read($reply,$read,2-$read)
        if ($count -eq 0) { throw 'SOCKS 응답 없음' }; $read += $count
    }
    ConvertTo-Json -InputObject ($reply[0] -eq 5 -and $reply[1] -eq 0) -Compress
} catch { ConvertTo-Json -InputObject $false -Compress }
finally { $client.Dispose() }
"#;

pub fn check_windows_proxy(port: u16) -> Result<(), String> {
    if powershell_json(PROBE_SCRIPT, json!({"port": port}))? == json!(true) {
        return Ok(());
    }
    Err(format!("Windows에서 WSL SOCKS5 프록시(127.0.0.1:{port})에 접속하지 못했습니다. WSL localhost 전달 설정과 Windows 쪽 동일 포트 사용 여부를 확인하세요."))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_wsl1_and_wsl2_without_classifying_regular_linux() {
        assert!(kernel_is_wsl("4.4.0-19041-Microsoft"));
        assert!(kernel_is_wsl("6.6.87.2-microsoft-standard-WSL2"));
        assert!(!kernel_is_wsl("6.8.0-71-generic"));
    }
    #[test]
    fn distinguishes_drive_absolute_paths_from_unc_and_relative_paths() {
        assert!(drive_path(r"D:\브라우저 폴더\firefox.exe"));
        assert!(drive_path("C:/Apps/msedge.exe"));
        for path in [
            "/mnt/c/Apps/browser.exe",
            "C:browser.exe",
            r"\\wsl.localhost\Ubuntu\home",
            "--flag",
        ] {
            assert!(!drive_path(path));
        }
    }
    #[test]
    fn encodes_request_data_without_leaving_shell_metacharacters() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        let encoded = base64("한글 ' $() ` & ;".as_bytes());
        assert!(encoded
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"+/=".contains(&byte)));
    }
}
