use crate::config::Config;
use crate::tunnel::TunnelManager;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
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

pub async fn run_web_server(state: AppState, port: u16) -> Result<(), String> {
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/assets/*path", get(asset_handler))
        .route("/api/status", get(get_status))
        .route("/api/logs", get(get_logs))
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
    use super::{web_listen_addr, WebAssets};

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
