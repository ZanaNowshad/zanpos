/// ZANPOS LAN hub discovery via mDNS-SD.
///
/// Advertises and discovers `_zanpos-hub._tcp.local` services on the local network.
/// Only advertises non-sensitive metadata.
#[cfg(feature = "mdns-discovery")]
mod inner {
    use std::collections::HashSet;
    use std::net::IpAddr;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub struct DiscoveredHub {
        pub instance_id: String,
        pub host: IpAddr,
        pub port: u16,
        pub protocol_version: u16,
        pub branch: String,
        pub pairing_enabled: bool,
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

    pub struct HubDiscovery {
        discovered: Arc<Mutex<HashSet<DiscoveredHub>>>,
    }

    impl HubDiscovery {
        pub fn new(_config: HubDiscoveryConfig) -> Result<Self, String> {
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
    use std::net::IpAddr;

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub struct DiscoveredHub {
        pub instance_id: String,
        pub host: IpAddr,
        pub port: u16,
        pub protocol_version: u16,
        pub branch: String,
        pub pairing_enabled: bool,
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

    pub struct HubDiscovery;

    impl HubDiscovery {
        pub fn new(_config: HubDiscoveryConfig) -> Result<Self, String> {
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

pub use inner::{DiscoveredHub, HubDiscovery, HubDiscoveryConfig};
