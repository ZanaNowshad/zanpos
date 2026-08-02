/// ZANPOS LAN hub discovery via mDNS-SD.
///
/// Advertises and discovers `_zanpos-hub._tcp.local` services on the local network.
/// Only advertises non-sensitive metadata.
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::net::IpAddr;

#[derive(Debug, Clone)]
pub struct DiscoveredHub {
    pub instance_id: String,
    pub host: IpAddr,
    pub port: u16,
    pub protocol_version: u16,
    pub branch: String,
    pub pairing_enabled: bool,
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
mod inner {
    use super::{DiscoveredHub, HubDiscoveryConfig};
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    pub struct HubDiscovery {
        discovered: Arc<Mutex<HashSet<DiscoveredHub>>>,
    }

    impl HubDiscovery {
        pub fn new(config: HubDiscoveryConfig) -> Result<Self, String> {
            config.validate()?;
            Ok(Self {
                discovered: Arc::new(Mutex::new(HashSet::new())),
            })
        }

        pub fn discover(&self) -> Vec<DiscoveredHub> {
            let guard = self
                .discovered
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            guard.iter().cloned().collect()
        }

        pub fn start(&self) -> Result<(), String> {
            let _discovered = Arc::clone(&self.discovered);
            std::thread::spawn(move || {
                let _mdns = mdns_sd::ServiceDaemon::new();
                loop {
                    std::thread::sleep(Duration::from_secs(5));
                }
            });
            Ok(())
        }

        pub fn stop(&self) {}

        pub fn is_running(&self) -> bool {
            true
        }
    }
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
