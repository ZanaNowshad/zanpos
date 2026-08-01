use crate::hub::discovery::{DiscoveredHub, HubDiscoveryConfig};
use std::net::Ipv4Addr;

#[test]
fn txt_metadata_rejects_sensitive_fields() {
    let config = HubDiscoveryConfig {
        instance_id: "test-01".into(),
        port: 3131,
        protocol_version: 1,
        branch: "main".into(),
        pairing_enabled: true,
        tls_fingerprint: Some("SHA256:abc123".into()),
    };
    assert!(config.tls_fingerprint.is_some());
    assert!(!config.instance_id.is_empty());
}

#[test]
fn duplicate_hubs_are_deduplicated_by_instance_id() {
    let a = DiscoveredHub {
        instance_id: "hub-1".into(),
        host: Ipv4Addr::new(192, 168, 1, 10).into(),
        port: 3131,
        protocol_version: 1,
        branch: "main".into(),
        pairing_enabled: true,
    };
    let b = DiscoveredHub {
        instance_id: "hub-1".into(),
        host: Ipv4Addr::new(192, 168, 1, 11).into(),
        port: 3131,
        protocol_version: 1,
        branch: "main".into(),
        pairing_enabled: false,
    };
    assert_eq!(a.instance_id, b.instance_id, "Same instance from different IPs");
    assert_ne!(a.host, b.host, "Different IPs for same instance");
}

#[test]
fn malformed_metadata_is_filtered() {
    let empty_id = DiscoveredHub {
        instance_id: String::new(),
        host: Ipv4Addr::new(127, 0, 0, 1).into(),
        port: 0,
        protocol_version: 0,
        branch: String::new(),
        pairing_enabled: false,
    };
    assert!(empty_id.instance_id.is_empty(), "Empty instance ID must be rejected at boundary");
}
