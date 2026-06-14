//! Embedded LAN hub server. When app_config.hub_mode='1', this device serves
//! its SQLite to sibling terminals over the PostgREST subset that
//! sync_v2::client::HttpSyncClient already speaks.
pub mod rest;

use crate::errors::{AppError, AppResult};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

/// device_id -> (remote_ip, last_seen RFC3339). Fed by auth middleware.
pub type SeenMap = Arc<Mutex<HashMap<String, (String, String)>>>;

#[derive(Clone)]
pub struct HubState {
    pub pool: SqlitePool,
    /// SHA-256 of the store token — raw token never lives in server memory.
    pub token_digest: [u8; 32],
    pub seen: SeenMap,
}

pub struct HubHandle {
    pub port: u16,
    pub seen: SeenMap,
    shutdown: tokio::sync::oneshot::Sender<()>,
}

impl HubHandle {
    pub fn shutdown(self) {
        let _ = self.shutdown.send(());
    }
}

pub fn token_digest(token: &str) -> [u8; 32] {
    let mut d = Sha256::new();
    d.update(token.as_bytes());
    d.finalize().into()
}

/// Bind 0.0.0.0:port (port 0 = ephemeral, used by tests) and serve.
pub async fn start_hub(pool: SqlitePool, port: u16, token: &str) -> AppResult<HubHandle> {
    let seen: SeenMap = Arc::new(Mutex::new(HashMap::new()));
    let state = HubState {
        pool,
        token_digest: token_digest(token),
        seen: seen.clone(),
    };
    let app = rest::router(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| AppError::Internal(format!("Hub: cannot bind port {port}: {e}")))?;
    let actual_port = listener.local_addr().map(|a| a.port()).unwrap_or(port);

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tauri::async_runtime::spawn(async move {
        let serve = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async {
            let _ = rx.await;
        });
        if let Err(e) = serve.await {
            tracing::error!("Hub server exited with error: {e}");
        } else {
            tracing::info!("Hub server stopped");
        }
    });
    tracing::info!("Hub server listening on 0.0.0.0:{actual_port}");
    Ok(HubHandle {
        port: actual_port,
        seen,
        shutdown: tx,
    })
}

/// Best-effort LAN IPv4 discovery without extra crates: a connected UDP socket
/// reveals the outbound interface address. No packets are sent.
pub fn lan_ips() -> Vec<String> {
    let mut out = Vec::new();
    for probe in ["8.8.8.8:80", "192.168.1.1:80", "10.0.0.1:80"] {
        if let Ok(s) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if s.connect(probe).is_ok() {
                if let Ok(a) = s.local_addr() {
                    let ip = a.ip().to_string();
                    if ip != "0.0.0.0" && !out.contains(&ip) {
                        out.push(ip);
                    }
                }
            }
        }
    }
    out
}
