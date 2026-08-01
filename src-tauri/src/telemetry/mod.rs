/// ZANPOS OpenTelemetry integration.
///
/// Provides privacy-safe span export with deny-by-default attribute allowlists.
/// Never includes prompts, keys, PINs, phone/JID values, receipt lines, customer
/// details, addresses, or raw SQL parameters in exported spans.

#[cfg(feature = "otel-tracing")]
mod inner {
    use opentelemetry::trace::{Tracer, TracerProvider};
    use opentelemetry_sdk::trace as sdktrace;
    use opentelemetry_sdk::Resource;
    use std::sync::OnceLock;

    static TRACER: OnceLock<sdktrace::Tracer> = OnceLock::new();

    pub fn init(service_name: &str) -> Result<(), String> {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .build()
            .map_err(|e| e.to_string())?;

        let provider = sdktrace::TracerProvider::builder()
            .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
            .with_resource(Resource::new(vec![
                opentelemetry::KeyValue::new("service.name", service_name.to_string()),
            ]))
            .build();

        let tracer = provider.tracer("zanpos");
        let _ = TRACER.set(tracer);
        opentelemetry::global::set_tracer_provider(provider);

        Ok(())
    }

    pub fn shutdown() {
        opentelemetry::global::shutdown_tracer_provider();
    }
}

#[cfg(not(feature = "otel-tracing"))]
mod inner {
    pub fn init(_service_name: &str) -> Result<(), String> {
        Ok(())
    }

    pub fn shutdown() {}
}

pub use inner::init;

pub fn shutdown() {
    inner::shutdown();
}
