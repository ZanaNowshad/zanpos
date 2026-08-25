//! Embedded LAN hub server. When app_config.hub_mode='1', this device serves
//! its SQLite to sibling terminals over the PostgREST subset that
//! sync_v2::client::HttpSyncClient already speaks.
pub mod discovery;
pub mod pairing;
pub mod rest;
mod rest_parity;

use crate::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HubChangeEvent {
    pub event_id: String,
    pub table: String,
    pub origin_device_id: String,
    pub changed_at: String,
}

#[derive(Clone)]
pub struct HubEventBus {
    sender: tokio::sync::broadcast::Sender<HubChangeEvent>,
}

impl HubEventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = tokio::sync::broadcast::channel(capacity.max(8));
        Self { sender }
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<HubChangeEvent> {
        self.sender.subscribe()
    }

    pub fn publish(&self, table: &str, origin_device_id: &str) {
        let _ = self.sender.send(HubChangeEvent {
            event_id: ulid::Ulid::new().to_string(),
            table: table.to_string(),
            origin_device_id: origin_device_id.to_string(),
            changed_at: chrono::Utc::now().to_rfc3339(),
        });
    }
}

/// device_id -> (remote_ip, last_seen RFC3339). Fed by auth middleware.
pub type SeenMap = Arc<Mutex<HashMap<String, (String, String)>>>;

#[derive(Clone)]
pub struct HubState {
    pub pool: SqlitePool,
    /// SHA-256 of the store token — raw token never lives in server memory.
    pub token_digest: [u8; 32],
    /// Live per-device pairings, device_id -> token digest. Snapshot rather
    /// than a query because `check_auth` is synchronous and runs on every
    /// request. Refreshed by `reload_paired` after any pair or revoke.
    pub paired: PairedMap,
    pub seen: SeenMap,
    pub events: HubEventBus,
}

pub type PairedMap = Arc<Mutex<pairing::PairedDevices>>;

pub struct HubHandle {
    pub port: u16,
    /// Shared with the running server, so a pair/revoke takes effect without
    /// restarting the hub and dropping every connected till.
    pub paired: PairedMap,
    pub seen: SeenMap,
    pub events: HubEventBus,
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
    let events = HubEventBus::new(256);
    // Loaded once here; refreshed by `reload_paired` after a pair or revoke,
    // so a revoked terminal stops authenticating without a hub restart.
    let paired: PairedMap = Arc::new(Mutex::new(pairing::load_live(&pool).await));
    let state = HubState {
        pool,
        token_digest: token_digest(token),
        paired: paired.clone(),
        seen: seen.clone(),
        events: events.clone(),
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
        paired,
        seen,
        events,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hub_event_bus_broadcasts_changed_table() {
        let bus = HubEventBus::new(8);
        let mut receiver = bus.subscribe();

        bus.publish("sales", "POS02");

        let event = receiver.recv().await.expect("hub change event");
        assert_eq!(event.table, "sales");
        assert_eq!(event.origin_device_id, "POS02");
        assert!(!event.event_id.is_empty());
    }
}
