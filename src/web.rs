use crate::config::Config;
use crate::browser::{self, BrowserKind, Launcher};
use crate::tunnel::TunnelManager;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Mutex<Config>>,
    pub tunnel: Arc<TunnelManager>,
}

#[derive(RustEmbed)]
#[folder = "web/dist/"]
struct WebAssets;

#[derive(Serialize)]
pub struct StatusResponse {
    pub version: &'static str,
    pub install_source: &'static str,
    pub ssh_alive: bool,
    pub http_alive: bool,
    pub ssh_target: String,
    pub http_port: u16,
    pub socks_port: u16,
    pub has_password: bool,
}

#[derive(Deserialize)]
pub struct UpdateConfigRequest {
    pub ssh_target: String,
    pub http_port: u16,
    pub socks_port: u16,
    pub no_proxy: String,
    pub ssh_password: Option<String>,
    pub ssh_key_path: Option<String>,
    pub auto_connect: Option<bool>,
    pub auto_open_browser: Option<bool>,
}

#[derive(Default, Deserialize)]
struct ConnectRequest {
    password: Option<String>,
}

#[derive(Deserialize)]
struct BrowserSettingsRequest { launcher: Option<Launcher> }

#[derive(Deserialize)]
struct BrowserLaunchRequest { url: Option<String> }

#[derive(Deserialize)]
struct DefaultBrowserRequest { browser: Option<BrowserKind> }

pub async fn run_web_server(state: AppState, port: u16) -> Result<(), String> {
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/assets/*path", get(asset_handler))
        .route("/api/status", get(get_status))
        .route("/api/logs", get(get_logs))
        .route("/api/browsers", get(get_browsers))
        .route("/api/browser-default", get(get_browser_default).post(save_browser_default))
        .route("/api/browser-default/launch", post(launch_browser_default))
        .route("/api/browsers/:kind/settings", post(save_browser_settings))
        .route("/api/browsers/:kind/launch", post(launch_browser))
        .route("/api/config", get(get_config).post(update_config))
        .route("/api/tunnel/connect", post(connect_tunnel))
        .route("/api/tunnel/disconnect", post(disconnect_tunnel))
        .route("/api/tunnel/restart", post(restart_tunnel))
        .with_state(state.clone());

    let addr = web_listen_addr(port);

    println!("[Web UI] Dashboard available at http://127.0.0.1:{}", port);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("Cannot bind web UI port {}: {}", port, e))?;

    let tunnel_for_shutdown = state.tunnel.clone();
    let shutdown = async move {
        #[cfg(unix)]
        {
            let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("SIGTERM handler");
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {},
                _ = terminate.recv() => {},
            }
        }
        #[cfg(not(unix))]
        let _ = tokio::signal::ctrl_c().await;
        crate::log_info!(
            "
[PK] 종료 신호 수신. SSH 터널 및 프록시를 종료합니다..."
        );
        let _ = tunnel_for_shutdown.stop_ssh().await;
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(|e| format!("Web server error: {}", e))?;

    crate::log_info!("[PK] 프록시 서비스가 정상 종료되었습니다.");
    Ok(())
}

fn web_listen_addr(port: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], port))
}

async fn index_handler() -> Response {
    serve_asset("index.html")
}

async fn asset_handler(Path(path): Path<String>) -> Response {
    serve_asset(&format!("assets/{path}"))
}

fn serve_asset(path: &str) -> Response {
    let Some(asset) = WebAssets::get(path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let content_type = match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    };
    Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(asset.data.into_owned()))
        .expect("valid static asset response")
}

async fn get_status(State(state): State<AppState>) -> Json<StatusResponse> {
    let cfg = state.config.lock().await.clone();
    let ssh_alive = state.tunnel.is_socks_alive().await;
    let http_alive = state.tunnel.is_http_alive().await;

    Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION"),
        install_source: if std::env::var("PK_INSTALL_SOURCE").as_deref() == Ok("npm") {
            "npm"
        } else {
            "native"
        },
        ssh_alive,
        http_alive,
        ssh_target: cfg.ssh_target,
        http_port: cfg.http_port,
        socks_port: cfg.socks_port,
        has_password: cfg
            .ssh_password
            .as_ref()
            .map(|s| !s.is_empty())
            .unwrap_or(false),
    })
}

async fn get_logs() -> Json<Vec<String>> {
    let logs = crate::logger::get_recent_logs(80);
    Json(logs)
}

async fn get_config(State(state): State<AppState>) -> Json<Config> {
    let cfg = state.config.lock().await.clone();
    Json(cfg)
}

async fn update_config(
    State(state): State<AppState>,
    Json(payload): Json<UpdateConfigRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let mut cfg = state.config.lock().await;
    cfg.ssh_target = payload.ssh_target.trim().to_string();
    cfg.http_port = payload.http_port;
    cfg.socks_port = payload.socks_port;
    cfg.no_proxy = payload.no_proxy.trim().to_string();
    cfg.ssh_password = payload.ssh_password.and_then(|s| {
        let t = s.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    });
    cfg.ssh_key_path = payload.ssh_key_path.and_then(|s| {
        let t = s.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    });
    if let Some(ac) = payload.auto_connect {
        cfg.auto_connect = ac;
    }
    if let Some(ab) = payload.auto_open_browser {
        cfg.auto_open_browser = ab;
    }
    // Browser settings may also have been changed by `pk browser set`.
    let browser_settings = Config::load();
    cfg.browsers = browser_settings.browsers;
    cfg.default_browser = browser_settings.default_browser;

    if let Err(e) = cfg.save() {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to save: {}", e),
        ));
    }

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "설정이 성공적으로 저장되었습니다."
    })))
}

type BrowserApiError = (StatusCode, String);

fn check_browser_origin(headers: &HeaderMap, web_port: u16) -> Result<(), BrowserApiError> {
    // These endpoints can start local programs. Reject requests from other sites.
    let allowed = |value: &str| {
        value == format!("http://127.0.0.1:{web_port}") || value == format!("http://localhost:{web_port}")
            || (cfg!(debug_assertions) && matches!(value, "http://127.0.0.1:5173" | "http://localhost:5173"))
    };
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("");
    if !allowed(&format!("http://{host}")) || headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) == Some("cross-site") {
        return Err((StatusCode::FORBIDDEN, "PK 로컬 대시보드에서 실행하세요.".into()));
    }
    if let Some(origin) = headers.get(header::ORIGIN) {
        if !origin.to_str().map(allowed).unwrap_or(false) {
            return Err((StatusCode::FORBIDDEN, "다른 사이트에서 브라우저를 실행할 수 없습니다.".into()));
        }
    }
    Ok(())
}

async fn get_browsers(State(state): State<AppState>) -> Result<Json<Vec<browser::BrowserInfo>>, BrowserApiError> {
    let mut cfg = state.config.lock().await.clone();
    let entries = tokio::task::spawn_blocking(move || {
        cfg.browsers = Config::load().browsers;
        browser::list(&cfg)
    }).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(entries))
}

async fn save_browser_settings(
    State(state): State<AppState>, Path(kind): Path<String>, headers: HeaderMap,
    Json(payload): Json<BrowserSettingsRequest>,
) -> Result<Json<serde_json::Value>, BrowserApiError> {
    check_browser_origin(&headers, state.config.lock().await.web_port)?;
    let kind = BrowserKind::parse(&kind).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    let launcher = tokio::task::spawn_blocking(move || payload.launcher.map(|launcher| browser::normalize_launcher(kind, launcher)).transpose())
        .await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    let mut cfg = state.config.lock().await;
    let mut updated = cfg.clone();
    let browser_settings = Config::load();
    updated.browsers = browser_settings.browsers;
    updated.default_browser = browser_settings.default_browser;
    if let Some(launcher) = launcher { updated.browsers.insert(kind, launcher); }
    else { updated.browsers.remove(&kind); }
    updated.save().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    *cfg = updated;
    Ok(Json(serde_json::json!({"status": "success"})))
}

async fn launch_browser(
    State(state): State<AppState>, Path(kind): Path<String>, headers: HeaderMap,
    Json(payload): Json<BrowserLaunchRequest>,
) -> Result<Json<serde_json::Value>, BrowserApiError> {
    let mut cfg = state.config.lock().await.clone();
    check_browser_origin(&headers, cfg.web_port)?;
    let kind = BrowserKind::parse(&kind).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    cfg.browsers = Config::load().browsers;
    let profile = browser::launch(&cfg, kind, payload.url).await.map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(Json(serde_json::json!({"status": "success", "profile": profile})))
}

async fn get_browser_default(State(state): State<AppState>) -> Result<Json<crate::browser_default::DefaultBrowserInfo>, BrowserApiError> {
    let mut cfg = state.config.lock().await.clone();
    let info = tokio::task::spawn_blocking(move || {
        cfg.default_browser = Config::load().default_browser;
        crate::browser_default::info(&cfg)
    }).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(info))
}

async fn save_browser_default(
    State(state): State<AppState>, headers: HeaderMap,
    Json(payload): Json<DefaultBrowserRequest>,
) -> Result<Json<serde_json::Value>, BrowserApiError> {
    check_browser_origin(&headers, state.config.lock().await.web_port)?;
    if payload.browser.is_some_and(|kind| !kind.supported()) {
        return Err((StatusCode::BAD_REQUEST, "Safari는 macOS에서만 선택할 수 있습니다.".into()));
    }
    let mut cfg = state.config.lock().await;
    let mut updated = cfg.clone();
    updated.browsers = Config::load().browsers;
    updated.default_browser = payload.browser;
    updated.save().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    *cfg = updated;
    Ok(Json(serde_json::json!({"status": "success"})))
}

async fn launch_browser_default(
    State(state): State<AppState>, headers: HeaderMap,
    Json(payload): Json<BrowserLaunchRequest>,
) -> Result<Json<serde_json::Value>, BrowserApiError> {
    let mut cfg = state.config.lock().await.clone();
    check_browser_origin(&headers, cfg.web_port)?;
    let browser_settings = Config::load();
    cfg.browsers = browser_settings.browsers;
    cfg.default_browser = browser_settings.default_browser;
    let (kind, profile) = crate::browser_default::launch(cfg, payload.url).await.map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(Json(serde_json::json!({"status": "success", "kind": kind, "profile": profile})))
}

async fn connect_tunnel(
    State(state): State<AppState>,
    payload: Option<Json<ConnectRequest>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let temporary_password = payload
        .and_then(|Json(request)| request.password)
        .filter(|password| !password.trim().is_empty());
    let result = match state.tunnel.connect_ssh(temporary_password).await {
        Ok(_) => match state.tunnel.wait_for_socks(30).await {
            Ok(_) => Ok(Json(serde_json::json!({
                "status": "success",
                "message": "SSH 터널이 성공적으로 연결되었습니다."
            }))),
            Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
        },
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    };
    if result.is_err() {
        state.tunnel.clear_temporary_password().await;
    }
    result
}

async fn disconnect_tunnel(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    match state.tunnel.stop_ssh().await {
        Ok(_) => {
            state.tunnel.clear_temporary_password().await;
            Ok(Json(serde_json::json!({
                "status": "success",
                "message": "SSH 터널 연결을 해제했습니다."
            })))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

async fn restart_tunnel(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    match state.tunnel.restart().await {
        Ok(_) => Ok(Json(serde_json::json!({
            "status": "success",
            "message": "터널 재연결을 완료했습니다."
        }))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

#[cfg(test)]
mod tests {
    use super::{check_browser_origin, web_listen_addr, WebAssets};

    #[test]
    fn browser_launch_rejects_foreign_origins_and_dns_rebinding() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("host", "127.0.0.1:8253".parse().unwrap());
        assert!(check_browser_origin(&headers, 8253).is_ok());
        headers.insert("origin", "https://example.com".parse().unwrap());
        assert!(check_browser_origin(&headers, 8253).is_err());
        headers.insert("origin", "http://127.0.0.1:8253".parse().unwrap());
        assert!(check_browser_origin(&headers, 8253).is_ok());
        headers.insert("host", "example.com:8253".parse().unwrap());
        assert!(check_browser_origin(&headers, 8253).is_err());
    }

    #[test]
    fn web_ui_only_binds_to_loopback() {
        let addr = web_listen_addr(8253);
        assert!(addr.ip().is_loopback());
        assert_eq!(addr.port(), 8253);
    }

    #[test]
    fn built_page_references_embedded_assets() {
        let index = WebAssets::get("index.html").expect("build the web app before cargo test");
        let html = String::from_utf8(index.data.into_owned()).expect("UTF-8 HTML");

        for marker in ["src=\"/assets/", "href=\"/assets/"] {
            let filename = html
                .split(marker)
                .nth(1)
                .and_then(|value| value.split('"').next())
                .expect("built JavaScript and CSS references");
            assert!(WebAssets::get(&format!("assets/{filename}")).is_some());
        }

        let rocket = WebAssets::iter()
            .find(|name| name.starts_with("assets/rocket-mark-") && name.ends_with(".png"))
            .expect("brand image is bundled with the web app");
        assert!(WebAssets::get(&rocket).is_some());
    }
}
