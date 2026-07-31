use super::*;
use crate::commands::cloudflare_commands::{CloudflareAccount, CloudflareConnection};

#[test]
fn settings_use_the_snake_case_contract_consumed_by_the_ui() {
    let value = serde_json::to_value(StorefrontSettings {
        enabled: true,
        public_url: "https://shop.example".into(),
        publish_url: "https://publish.example".into(),
        whatsapp_number: "97333001234".into(),
        locale: "en-ar".into(),
        auto_publish: false,
        publish_secret: None,
    })
    .unwrap();

    assert_eq!(value["public_url"], "https://shop.example");
    assert!(value.get("publicUrl").is_none());
    assert!(value.get("publish_secret").is_none());
}

#[test]
fn disabled_storefront_can_save_incomplete_setup() {
    let settings = StorefrontSettings {
        enabled: false,
        public_url: String::new(),
        publish_url: String::new(),
        whatsapp_number: String::new(),
        locale: "en-ar".into(),
        auto_publish: false,
        publish_secret: None,
    };

    assert!(validate_settings(&settings).is_ok());
}

#[test]
fn undeployed_managed_storefront_tests_cloudflare_instead_of_an_empty_url() {
    assert!(uses_cloudflare_connection_test(""));
    assert!(uses_cloudflare_connection_test("   "));
    assert!(!uses_cloudflare_connection_test("https://shop.example"));
}

#[test]
fn cloudflare_connection_never_serializes_a_credential() {
    let value = serde_json::to_value(CloudflareConnection {
        state: "connected".into(),
        account_id: Some("acc-1".into()),
        account_name: Some("Pearl Market".into()),
        accounts: vec![CloudflareAccount {
            id: "acc-1".into(),
            name: "Pearl Market".into(),
        }],
        credential_stored: true,
        last_verified_at: Some("2026-07-23T12:00:00Z".into()),
        issue: None,
    })
    .unwrap();

    assert!(value.get("api_token").is_none());
    assert!(value.get("access_token").is_none());
    assert_eq!(value["credential_stored"], true);
}

#[test]
fn snapshot_drift_keeps_removals_visible_to_storefront_status() {
    assert_eq!(effective_dirty_count(0, Some("published"), "current"), 1);
    assert_eq!(effective_dirty_count(3, Some("published"), "current"), 3);
    assert_eq!(effective_dirty_count(0, Some("same"), "same"), 0);
    assert_eq!(effective_dirty_count(0, None, "current"), 0);
}
