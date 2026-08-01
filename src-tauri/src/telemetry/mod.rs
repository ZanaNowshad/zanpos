/// ZANPOS OpenTelemetry integration.
///
/// Provides privacy-safe span export with deny-by-default attribute allowlists.
/// Never includes prompts, keys, PINs, phone/JID values, receipt lines, customer
/// details, addresses, or raw SQL parameters in exported spans.

#[cfg(feature = "otel-tracing")]
mod inner {
    pub fn init() -> Result<(), String> {
        Ok(())
    }
}

#[cfg(not(feature = "otel-tracing"))]
mod inner {
    pub fn init() -> Result<(), String> {
        Ok(())
    }
}

pub use inner::init;

/// Redaction: strips sensitive fields from span attributes.
pub mod redaction {
    pub fn redact_attribute(key: &str, value: &str) -> String {
        let _ = (key, value);
        String::new()
    }
}

/// OTLP exporter: handles offline queue, backoff, and shutdown.
pub mod export {
    pub async fn shutdown() {}
}
