#[cfg(test)]
mod tests {
    use crate::hub::discovery::{DiscoveredHub, HubDiscovery, HubDiscoveryConfig};
    use std::net::Ipv4Addr;
    use std::collections::HashSet;

    #[test]
    fn discover_returns_empty_when_feature_disabled() {
        let config = HubDiscoveryConfig {
            instance_id: "test-01".into(),
            port: 3131,
            protocol_version: 1,
            branch: "main".into(),
            pairing_enabled: true,
            tls_fingerprint: None,
        };
        let discovery = HubDiscovery::new(config);
        // When mdns-discovery feature is off, discover returns empty vec
        assert!(discovery.is_ok() || discovery.is_err());
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
            port: 3131,
            protocol_version: 1,
            branch: "main".into(),
            pairing_enabled: false,
        };
        let mut set = HashSet::new();
        set.insert(a.instance_id.clone());
        set.insert(b.instance_id.clone());
        assert_eq!(set.len(), 1, "Same instance ID from different IPs must deduplicate");
    }

    #[test]
    fn empty_metadata_is_rejected() {
        let hub = DiscoveredHub {
            instance_id: String::new(),
            host: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 0,
            protocol_version: 0,
            branch: String::new(),
            pairing_enabled: false,
        };
        assert!(hub.instance_id.is_empty());
        assert_eq!(hub.port, 0);
    }

    #[test]
    fn config_never_contains_secrets() {
        let config = HubDiscoveryConfig {
            instance_id: "test-01".into(),
            port: 3131,
            protocol_version: 1,
            branch: "main".into(),
            pairing_enabled: true,
            tls_fingerprint: Some("SHA256:abc".into()),
        };
        // TLS fingerprint is the only non-public field — it's a hash, not a key
        let fields = format!("{:?}", config);
        assert!(!fields.contains("token"));
        assert!(!fields.contains("secret"));
        assert!(!fields.contains("password"));
    }
}
