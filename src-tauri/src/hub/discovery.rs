/// ZANPOS LAN hub discovery via mDNS-SD.
///
/// Advertises and discovers `_zanpos-hub._tcp.local` services on the local network.
/// Only advertises non-sensitive metadata.
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::net::IpAddr;

#[allow(dead_code)]
const SERVICE_TYPE: &str = "_zanpos-hub._tcp.local.";
#[allow(dead_code)]
const STALE_DURATION_SECS: u64 = 30;
#[allow(dead_code)]
const PROTOCOL_VERSION_MIN: u16 = 1;

#[derive(Debug, Clone)]
pub struct DiscoveredHub {
    pub instance_id: String,
    pub host: IpAddr,
    pub port: u16,
    pub protocol_version: u16,
    pub branch: String,
    pub pairing_enabled: bool,
    pub tls_fingerprint: Option<String>,
    pub last_seen_secs: u64,
}

impl DiscoveredHub {
    pub fn validate(&self) -> Result<(), String> {
        validate_text("instance_id", &self.instance_id, 128)?;
        validate_text("branch", &self.branch, 128)?;
        if self.host.is_unspecified() {
            return Err("host must be a specific IP address".into());
        }
        validate_network_values(self.port, self.protocol_version)
    }
}

impl PartialEq for DiscoveredHub {
    fn eq(&self, other: &Self) -> bool {
        self.instance_id == other.instance_id
    }
}

impl Eq for DiscoveredHub {}

impl Hash for DiscoveredHub {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.instance_id.hash(state);
    }
}

#[derive(Debug, Clone)]
pub struct HubDiscoveryConfig {
    pub instance_id: String,
    pub port: u16,
    pub protocol_version: u16,
    pub branch: String,
    pub pairing_enabled: bool,
    pub tls_fingerprint: Option<String>,
}

impl HubDiscoveryConfig {
    pub fn validate(&self) -> Result<(), String> {
        validate_text("instance_id", &self.instance_id, 128)?;
        validate_text("branch", &self.branch, 128)?;
        validate_network_values(self.port, self.protocol_version)?;
        if let Some(fingerprint) = &self.tls_fingerprint {
            validate_text("tls_fingerprint", fingerprint, 256)?;
        }
        Ok(())
    }

    pub fn advertised_metadata(&self) -> BTreeMap<String, String> {
        let mut metadata = BTreeMap::from([
            ("branch".into(), self.branch.clone()),
            ("instance_id".into(), self.instance_id.clone()),
            ("pairing_enabled".into(), self.pairing_enabled.to_string()),
            ("protocol_version".into(), self.protocol_version.to_string()),
        ]);
        if let Some(fingerprint) = &self.tls_fingerprint {
            metadata.insert("tls_fingerprint".into(), fingerprint.clone());
        }
        metadata
    }
}

fn validate_text(field: &str, value: &str, max_len: usize) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    if value.len() > max_len {
        return Err(format!("{field} must not exceed {max_len} bytes"));
    }
    Ok(())
}

fn validate_network_values(port: u16, protocol_version: u16) -> Result<(), String> {
    if port == 0 {
        return Err("port must be non-zero".into());
    }
    if protocol_version == 0 {
        return Err("protocol_version must be non-zero".into());
    }
    Ok(())
}

#[cfg(feature = "mdns-discovery")]
fn parse_txt_record(properties: &BTreeMap<String, String>) -> Result<DiscoveredHub, String> {
    let instance_id = properties
        .get("instance_id")
        .ok_or("missing instance_id")?
        .clone();
    let branch = properties.get("branch").ok_or("missing branch")?.clone();
    let protocol_version: u16 = properties
        .get("protocol_version")
        .ok_or("missing protocol_version")?
        .parse()
        .map_err(|_| "invalid protocol_version")?;
    let pairing_enabled = properties
        .get("pairing_enabled")
        .map(|v| v == "true")
        .unwrap_or(false);
    let tls_fingerprint = properties.get("tls_fingerprint").cloned();

    if protocol_version < PROTOCOL_VERSION_MIN {
        return Err(format!(
            "protocol_version {protocol_version} below minimum {PROTOCOL_VERSION_MIN}"
        ));
    }

    Ok(DiscoveredHub {
        instance_id,
        host: std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), // filled in by caller
        port: 0,
        protocol_version,
        branch,
        pairing_enabled,
        tls_fingerprint,
        last_seen_secs: 0,
    })
}

#[cfg(feature = "mdns-discovery")]
mod inner {
    use super::{
        parse_txt_record, DiscoveredHub, HubDiscoveryConfig, SERVICE_TYPE, STALE_DURATION_SECS,
    };
    use std::collections::{BTreeMap, HashSet};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    pub struct HubDiscovery {
        discovered: Arc<Mutex<HashSet<DiscoveredHub>>>,
        running: Arc<AtomicBool>,
        daemon: Option<Arc<mdns_sd::ServiceDaemon>>,
    }

    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    impl HubDiscovery {
        pub fn new(config: HubDiscoveryConfig) -> Result<Self, String> {
            config.validate()?;
            let daemon = mdns_sd::ServiceDaemon::new()
                .map_err(|e| format!("mDNS daemon creation failed: {e}"))?;
            let daemon = Arc::new(daemon);

            // Register our own service
            let txt_properties: Vec<(&str, &str)> = vec![("txtvers", "1")];
            let service_info = mdns_sd::ServiceInfo::new(
                SERVICE_TYPE,
                &config.instance_id,
                &format!("zanpos-hub-{}.local.", &config.instance_id),
                "",
                config.port,
                txt_properties.as_slice(),
            )
            .map_err(|e| format!("mDNS service info creation failed: {e}"))?;

            daemon
                .register(service_info)
                .map_err(|e| format!("mDNS registration failed: {e}"))?;

            Ok(Self {
                discovered: Arc::new(Mutex::new(HashSet::new())),
                running: Arc::new(AtomicBool::new(false)),
                daemon: Some(daemon),
            })
        }

        pub fn discover(&self) -> Vec<DiscoveredHub> {
            let guard = self
                .discovered
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let now = now_secs();
            guard
                .iter()
                .filter(|hub| now.saturating_sub(hub.last_seen_secs) <= STALE_DURATION_SECS)
                .cloned()
                .collect()
        }

        pub fn start(&self) -> Result<(), String> {
            if self
                .running
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                return Ok(());
            }

            let discovered = Arc::clone(&self.discovered);
            let running = Arc::clone(&self.running);
            let daemon = Arc::clone(self.daemon.as_ref().ok_or("mDNS daemon not initialized")?);

            // Browse for _zanpos-hub._tcp.local services
            let receiver = daemon
                .browse(SERVICE_TYPE)
                .map_err(|e| format!("mDNS browse failed: {e}"))?;

            std::thread::spawn(move || {
                while running.load(Ordering::SeqCst) {
                    match receiver.recv_timeout(Duration::from_secs(2)) {
                        Ok(mdns_sd::ServiceEvent::ServiceResolved(info)) => {
                            let fullname = info.get_fullname().to_string();
                            // Skip our own service
                            if fullname.starts_with("zanpos-hub-") {
                                continue;
                            }
                            Self::handle_resolved(&discovered, &info);
                        }
                        Ok(_) => {} // Ignore other events (search started, etc.)
                        Err(_) => {
                            // recv_timeout exhausted or daemon disconnected.
                            // Loop continues; running flag controls lifecycle.
                        }
                    }
                }
            });

            Ok(())
        }

        fn handle_resolved(
            discovered: &Arc<Mutex<HashSet<DiscoveredHub>>>,
            info: &mdns_sd::ServiceInfo,
        ) {
            let raw_props = info.get_properties();
            let properties: BTreeMap<String, String> = raw_props
                .iter()
                .map(|prop| {
                    let key = prop.key().to_string();
                    let value = prop.val_str().to_string();
                    (key, value)
                })
                .collect();
            let timestamp = now_secs();

            let mut hub = match parse_txt_record(&properties) {
                Ok(hub) => hub,
                Err(_) => return, // Malformed TXT — silently rejected
            };

            // Fill in connection details from the resolved service
            hub.port = info.get_port();
            hub.last_seen_secs = timestamp;

            // Use first resolved address
            if let Some(addr) = info.get_addresses().iter().next() {
                hub.host = *addr;
            }

            let mut guard = discovered.lock().unwrap_or_else(|error| error.into_inner());

            // Update or insert — deduplicates by instance_id
            guard.replace(hub);
        }

        pub fn stop(&self) {
            self.running.store(false, Ordering::SeqCst);
        }

        pub fn is_running(&self) -> bool {
            self.running.load(Ordering::SeqCst)
        }
    }

    // Safety: the daemon is used from a single thread at a time
    unsafe impl Send for HubDiscovery {}
    unsafe impl Sync for HubDiscovery {}
}

#[cfg(not(feature = "mdns-discovery"))]
mod inner {
    use super::{DiscoveredHub, HubDiscoveryConfig};

    pub struct HubDiscovery;

    impl HubDiscovery {
        pub fn new(config: HubDiscoveryConfig) -> Result<Self, String> {
            config.validate()?;
            Err("mDNS discovery not compiled in — enable the mdns-discovery feature".into())
        }

        pub fn discover(&self) -> Vec<DiscoveredHub> {
            vec![]
        }

        pub fn start(&self) -> Result<(), String> {
            Ok(())
        }

        pub fn stop(&self) {}

        pub fn is_running(&self) -> bool {
            false
        }
    }
}

pub use inner::HubDiscovery;

#[cfg(test)]
mod tests {
    use super::{DiscoveredHub, HubDiscovery, HubDiscoveryConfig};
    use std::collections::HashSet;
    use std::net::{IpAddr, Ipv4Addr};

    fn valid_config() -> HubDiscoveryConfig {
        HubDiscoveryConfig {
            instance_id: "test-01".into(),
            port: 3131,
            protocol_version: 1,
            branch: "main".into(),
            pairing_enabled: true,
            tls_fingerprint: Some("SHA256:abc".into()),
        }
    }

    #[test]
    fn discover_returns_empty_when_feature_disabled() {
        let result = HubDiscovery::new(valid_config());

        #[cfg(not(feature = "mdns-discovery"))]
        assert_eq!(
            result.err().as_deref(),
            Some("mDNS discovery not compiled in — enable the mdns-discovery feature")
        );

        #[cfg(feature = "mdns-discovery")]
        assert!(result
            .expect("valid discovery config")
            .discover()
            .is_empty());
    }

    #[test]
    fn discovered_hubs_dedup_by_instance_id() {
        let a = DiscoveredHub {
            instance_id: "hub-1".into(),
            host: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)),
            port: 3131,
            protocol_version: 1,
            branch: "main".into(),
            pairing_enabled: true,
            tls_fingerprint: None,
            last_seen_secs: 0,
        };
        let b = DiscoveredHub {
            instance_id: "hub-1".into(),
            host: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 11)),
            pairing_enabled: false,
            ..a.clone()
        };

        let hubs = HashSet::from([a, b]);

        assert_eq!(hubs.len(), 1, "instance ID is the discovery identity");
    }

    #[test]
    fn empty_metadata_is_rejected() {
        let invalid_hub = DiscoveredHub {
            instance_id: String::new(),
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            protocol_version: 0,
            branch: String::new(),
            pairing_enabled: false,
            tls_fingerprint: None,
            last_seen_secs: 0,
        };

        assert!(invalid_hub.validate().is_err());

        let mut invalid_config = valid_config();
        invalid_config.instance_id.clear();
        assert!(HubDiscovery::new(invalid_config).is_err());
    }

    #[test]
    fn config_never_contains_secrets() {
        let metadata = valid_config().advertised_metadata();
        let keys: Vec<_> = metadata.keys().map(String::as_str).collect();

        assert_eq!(
            keys,
            [
                "branch",
                "instance_id",
                "pairing_enabled",
                "protocol_version",
                "tls_fingerprint",
            ]
        );
        assert_eq!(
            metadata.get("tls_fingerprint").map(String::as_str),
            Some("SHA256:abc")
        );
        assert!(keys
            .iter()
            .all(|key| !matches!(*key, "token" | "secret" | "password" | "private_key")));
    }
}
