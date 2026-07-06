use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use reqwest::Client;
use serde_json::Value;
use std::env;
use std::net::SocketAddr;
use tower_http::services::{ServeDir, ServeFile};
use tracing::info;

#[derive(Clone)]
struct AppState {
    client: Client,
    portainer_url: String,
    api_key: String,
    endpoint_id: String,
    server_local_ip: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let portainer_url = env::var("PORTAINER_URL")
        .unwrap_or_default()
        .trim_end_matches('/')
        .to_string();
    let api_key = env::var("PORTAINER_API_KEY").unwrap_or_default();
    let endpoint_id = env::var("PORTAINER_ENDPOINT_ID").unwrap_or_else(|_| "1".to_string());
    let port: u16 = env::var("PORT")
        .unwrap_or_else(|_| "3456".to_string())
        .parse()
        .unwrap_or(3456);

    let server_local_ip = env::var("SERVER_LOCAL_IP").unwrap_or_else(|_| "".to_string());

    let state = AppState {
        client: Client::new(),
        portainer_url,
        api_key,
        endpoint_id,
        server_local_ip,
    };

    let api_routes = Router::new()
        .route("/health", get(health_handler))
        .route("/stacks", get(get_stacks))
        .route("/stacks/:id/start", post(start_stack))
        .route("/stacks/:id/stop", post(stop_stack))
        .route("/containers", get(get_containers))
        .route("/containers/:id/start", post(start_container))
        .route("/containers/:id/stop", post(stop_container))
        .route("/containers/:id/restart", post(restart_container))
        .route("/settings", get(get_settings).post(save_settings))
        .route("/config", get(get_config))
        .with_state(state);

    let app = Router::new()
        .nest("/api", api_routes)
        .fallback_service(
            ServeDir::new("public").fallback(ServeFile::new("public/index.html")),
        );

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("⚡ Plustainer Rust backend running on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// Helpers
fn get_headers(api_key: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("X-API-Key", api_key.parse().unwrap());
    headers.insert("Content-Type", "application/json".parse().unwrap());
    headers
}

async fn health_handler(State(state): State<AppState>) -> impl IntoResponse {
    let url = format!("{}/api/system/status", state.portainer_url);
    match state.client.get(&url).headers(get_headers(&state.api_key)).send().await {
        Ok(res) if res.status().is_success() => Json(serde_json::json!({ "connected": true })),
        _ => Json(serde_json::json!({ "connected": false })),
    }
}

async fn get_stacks(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    let url = format!("{}/api/stacks", state.portainer_url);
    let res = state.client.get(&url).headers(get_headers(&state.api_key)).send().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let json = res.json::<Value>().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json))
}

async fn start_stack(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let url = format!("{}/api/stacks/{}/start?endpointId={}", state.portainer_url, id, state.endpoint_id);
    let res = state.client.post(&url).headers(get_headers(&state.api_key)).send().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let json = res.json::<Value>().await.unwrap_or(serde_json::json!({}));
    Ok(Json(json))
}

async fn stop_stack(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let url = format!("{}/api/stacks/{}/stop?endpointId={}", state.portainer_url, id, state.endpoint_id);
    let res = state.client.post(&url).headers(get_headers(&state.api_key)).send().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let json = res.json::<Value>().await.unwrap_or(serde_json::json!({}));
    Ok(Json(json))
}

async fn get_containers(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    let url = format!("{}/api/endpoints/{}/docker/containers/json?all=true", state.portainer_url, state.endpoint_id);
    let res = state.client.get(&url).headers(get_headers(&state.api_key)).send().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let json = res.json::<Value>().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json))
}

async fn start_container(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let url = format!("{}/api/endpoints/{}/docker/containers/{}/start", state.portainer_url, state.endpoint_id, id);
    let res = state.client.post(&url).headers(get_headers(&state.api_key)).send().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if res.status().is_success() || res.status().as_u16() == 304 {
        Ok(Json(serde_json::json!({ "success": true })))
    } else {
        Err(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

async fn stop_container(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let url = format!("{}/api/endpoints/{}/docker/containers/{}/stop", state.portainer_url, state.endpoint_id, id);
    let res = state.client.post(&url).headers(get_headers(&state.api_key)).send().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if res.status().is_success() || res.status().as_u16() == 304 {
        Ok(Json(serde_json::json!({ "success": true })))
    } else {
        Err(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

async fn restart_container(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    let url = format!("{}/api/endpoints/{}/docker/containers/{}/restart", state.portainer_url, state.endpoint_id, id);
    let res = state.client.post(&url).headers(get_headers(&state.api_key)).send().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if res.status().is_success() {
        Ok(Json(serde_json::json!({ "success": true })))
    } else {
        Err(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

async fn get_settings() -> Result<Json<Value>, StatusCode> {
    let path = std::path::Path::new("/app/data/settings.json");
    if path.exists() {
        if let Ok(content) = tokio::fs::read_to_string(path).await {
            if let Ok(json) = serde_json::from_str(&content) {
                return Ok(Json(json));
            }
        }
    }
    // Return empty settings by default
    Ok(Json(serde_json::json!({
        "favorites": [],
        "custom_order": [],
        "sort_order": "custom",
        "pages": []
    })))
}

async fn save_settings(Json(payload): Json<Value>) -> Result<Json<Value>, StatusCode> {
    let path = std::path::Path::new("/app/data/settings.json");
    
    // Ensure directory exists
    if let Some(parent) = path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    
    let content = serde_json::to_string(&payload).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tokio::fs::write(path, content).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "success": true })))
}

async fn get_config(State(state): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "server_local_ip": state.server_local_ip
    }))
}
