use serde_json::{json, Value};
use std::{
    env, fs,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
};

static STARTING: AtomicBool = AtomicBool::new(false);

fn folder() -> PathBuf {
    crate::config::config_path()
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("npm-update")
}

pub fn job() -> Option<Value> {
    serde_json::from_slice(&fs::read(folder().join("job.json")).ok()?).ok()
}

fn check() -> Result<Value, String> {
    if env::var("PK_INSTALL_SOURCE").as_deref() != Ok("npm") {
        return Err("npm으로 설치한 PK에서 UI 업데이트를 사용할 수 있습니다. 먼저 npm install -g @gomul82/pk@latest를 실행하세요.".into());
    }
    let node =
        env::var_os("PK_NPM_NODE").ok_or("UI 업데이트를 지원하는 npm 버전으로 다시 실행하세요.")?;
    let root =
        PathBuf::from(env::var_os("PK_NPM_ROOT").ok_or("npm 설치 위치를 확인하지 못했습니다.")?);
    let mut command = Command::new(node);
    command
        .arg(root.join("updater.cjs"))
        .arg("check")
        .stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command
        .output()
        .map_err(|e| format!("Node.js 실행 실패: {e}"))?;
    if !output.status.success() {
        return Err(
            "npm 업데이트 도구를 실행하지 못했습니다. npm으로 PK를 다시 설치하세요.".into(),
        );
    }
    let result: Value =
        serde_json::from_slice(&output.stdout).map_err(|_| "npm 설치 정보를 읽지 못했습니다.")?;
    if result["available"] != true {
        return Err(result["reason"]
            .as_str()
            .unwrap_or("npm 업데이트를 사용할 수 없습니다.")
            .into());
    }
    Ok(result)
}

pub fn info() -> Value {
    match check() {
        Ok(_) => json!({"available": true, "reason": null, "job": public_job()}),
        Err(reason) => json!({"available": false, "reason": reason, "job": public_job()}),
    }
}

fn public_job() -> Option<Value> {
    job().map(|job| json!({"id": job["id"], "phase": job["phase"], "message": job["message"]}))
}

pub fn begin(port: u16, http_port: u16, socks_port: u16) -> Result<Value, String> {
    if STARTING.swap(true, Ordering::SeqCst) {
        return Err("이미 업데이트가 진행 중입니다.".into());
    }
    let result = prepare(port, http_port, socks_port);
    STARTING.store(false, Ordering::SeqCst);
    result
}

fn prepare(port: u16, http_port: u16, socks_port: u16) -> Result<Value, String> {
    if let Some(job) = job() {
        if matches!(
            job["phase"].as_str(),
            Some("starting" | "ready" | "installing" | "restarting")
        ) {
            let pid = fs::read_to_string(folder().join("helper.pid"))
                .ok()
                .and_then(|pid| pid.parse::<u32>().ok());
            if pid.is_some_and(crate::daemon::is_pid_alive) {
                return Err("이미 업데이트가 진행 중입니다.".into());
            }
        }
    }
    let mut job = check()?;
    let root = PathBuf::from(job["root"].as_str().ok_or("npm 설치 위치가 없습니다.")?);
    let folder = folder();
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    // Run outside node_modules so npm can replace the installed package.
    for name in ["updater.cjs", "launcher.cjs"] {
        fs::copy(root.join(name), folder.join(name)).map_err(|e| e.to_string())?;
    }
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos()
        .to_string();
    let config = crate::config::config_path();
    let config_dir = fs::canonicalize(config.parent().unwrap_or(std::path::Path::new(".")))
        .map_err(|e| e.to_string())?;
    job["id"] = json!(id);
    job["pid"] = json!(std::process::id());
    job["port"] = json!(port);
    job["proxy_ports"] = json!([http_port, socks_port]);
    job["config_dir"] = json!(config_dir);
    job["phase"] = json!("starting");
    job["message"] = json!("업데이트를 준비하는 중…");
    let job_file = folder.join("job.json");
    let mut command = Command::new(job["node"].as_str().ok_or("Node.js 위치가 없습니다.")?);
    command
        .arg(folder.join("updater.cjs"))
        .arg("run")
        .arg(&job_file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    fs::write(
        &job_file,
        serde_json::to_vec(&job).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let child = command.spawn().map_err(|e| e.to_string())?;
    // Do not rewrite job.json here: the helper may have published its ready marker.
    fs::write(folder.join("helper.pid"), child.id().to_string()).map_err(|e| e.to_string())?;
    std::thread::spawn(move || {
        let mut child = child;
        let _ = child.wait();
    });
    Ok(json!({"id": id, "phase": "starting", "message": "npm으로 업데이트하는 중…"}))
}

pub fn ready() -> bool {
    job().is_some_and(|job| job["phase"] == "ready")
}
