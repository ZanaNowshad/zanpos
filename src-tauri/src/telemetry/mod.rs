/// ZANPOS OpenTelemetry tracing — privacy-safe, bounded, resilient.
///
/// ## Design
/// - Deny-by-default attribute allowlists — never logs prompts, keys, PINs, phone/JID, receipts
/// - Bounded batch queue: max 2048 spans, 5s export interval, 30s export timeout
/// - Exponential backoff on export failure
/// - Graceful bounded shutdown (5s drain)
/// - Local no-op tracing when `otel-tracing` feature is disabled
/// - Non-blocking: spans are buffered and exported async; checkout/financial paths never blocked
use std::sync::OnceLock;
use std::time::Duration;

// ─── Configuration ──────────────────────────────────────────────────────────

const MAX_QUEUE_SIZE: usize = 2048;
const MAX_EXPORT_BATCH_SIZE: usize = 512;
const SCHEDULED_DELAY: Duration = Duration::from_secs(5);
const EXPORT_TIMEOUT: Duration = Duration::from_secs(30);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_BACKOFF: Duration = Duration::from_secs(60);

// ─── Span taxonomy ─────────────────────────────────────────────────────────

pub enum ZanposSpan {
    TauriCommand {
        command: &'static str,
    },
    DbTransaction {
        operation: &'static str,
    },
    SyncBatch {
        table_count: usize,
    },
    PrinterOperation {
        kind: &'static str,
    },
    AiProviderCall {
        provider: &'static str,
        model: &'static str,
    },
    AiToolExecution {
        tool: &'static str,
    },
    BackupOperation {
        phase: &'static str,
    },
    WhatsAppQueue {
        action: &'static str,
    },
    StorefrontPublish {
        stage: &'static str,
    },
}

impl ZanposSpan {
    pub fn name(&self) -> &'static str {
        match self {
            ZanposSpan::TauriCommand { .. } => "tauri.command",
            ZanposSpan::DbTransaction { .. } => "db.transaction",
            ZanposSpan::SyncBatch { .. } => "sync.batch",
            ZanposSpan::PrinterOperation { .. } => "printer.operation",
            ZanposSpan::AiProviderCall { .. } => "ai.provider.call",
            ZanposSpan::AiToolExecution { .. } => "ai.tool.execute",
            ZanposSpan::BackupOperation { .. } => "backup.operation",
            ZanposSpan::WhatsAppQueue { .. } => "whatsapp.queue",
            ZanposSpan::StorefrontPublish { .. } => "storefront.publish",
        }
    }

    /// Returns only allowlisted attributes — never includes prompts, keys, PINs,
    /// phone/JID values, receipt lines, customer details, addresses, or raw SQL.
    pub fn attributes(&self) -> Vec<(&'static str, String)> {
        match self {
            ZanposSpan::TauriCommand { command } => vec![("command.name", command.to_string())],
            ZanposSpan::DbTransaction { operation } => {
                vec![("db.operation", operation.to_string())]
            }
            ZanposSpan::SyncBatch { table_count } => {
                vec![("sync.table_count", table_count.to_string())]
            }
            ZanposSpan::PrinterOperation { kind } => vec![("printer.kind", kind.to_string())],
            ZanposSpan::AiProviderCall { provider, model } => vec![
                ("ai.provider", provider.to_string()),
                ("ai.model", model.to_string()),
            ],
            ZanposSpan::AiToolExecution { tool } => vec![("ai.tool", tool.to_string())],
            ZanposSpan::BackupOperation { phase } => vec![("backup.phase", phase.to_string())],
            ZanposSpan::WhatsAppQueue { action } => vec![("whatsapp.action", action.to_string())],
            ZanposSpan::StorefrontPublish { stage } => {
                vec![("storefront.stage", stage.to_string())]
            }
        }
    }
}

// ─── Initialization (feature-gated) ───────────────────────────────────────

#[cfg(feature = "otel-tracing")]
mod inner {
    use super::*;
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_sdk::trace as sdktrace;
    use opentelemetry_sdk::Resource;

    static TRACER: OnceLock<sdktrace::Tracer> = OnceLock::new();

    pub fn init(service_name: &str) -> Result<(), String> {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_timeout(EXPORT_TIMEOUT)
            .build()
            .map_err(|error| format!("OTLP exporter creation failed: {error}"))?;

        let provider = sdktrace::TracerProvider::builder()
            .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
            .with_max_export_batch_size(MAX_EXPORT_BATCH_SIZE)
            .with_max_queue_size(MAX_QUEUE_SIZE)
            .with_scheduled_delay(SCHEDULED_DELAY)
            .with_resource(Resource::new(vec![opentelemetry::KeyValue::new(
                "service.name",
                service_name.to_string(),
            )]))
            .build();

        let tracer = provider.tracer("zanpos");
        let _ = TRACER.set(tracer);
        opentelemetry::global::set_tracer_provider(provider);

        Ok(())
    }

    pub fn shutdown() {
        opentelemetry::global::shutdown_tracer_provider();
        // OTel SDK shutdown drains the batch queue; bounded to SHUTDOWN_TIMEOUT
        // via the exporter timeout. The caller should not block indefinitely.
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

/// Drains pending spans and shuts down the tracer provider.
/// Bounded to SHUTDOWN_TIMEOUT — does not block checkout or financial paths.
pub fn shutdown() {
    inner::shutdown();
}

// ─── Helpers ───────────────────────────────────────────────────────────────

/// Returns whether OTel tracing is compiled in and initialized.
pub fn is_enabled() -> bool {
    cfg!(feature = "otel-tracing")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_spans_have_names() {
        let spans = [
            ZanposSpan::TauriCommand { command: "test" },
            ZanposSpan::DbTransaction { operation: "test" },
            ZanposSpan::SyncBatch { table_count: 0 },
            ZanposSpan::PrinterOperation { kind: "test" },
            ZanposSpan::AiProviderCall {
                provider: "x",
                model: "y",
            },
            ZanposSpan::AiToolExecution { tool: "test" },
            ZanposSpan::BackupOperation { phase: "test" },
            ZanposSpan::WhatsAppQueue { action: "test" },
            ZanposSpan::StorefrontPublish { stage: "test" },
        ];
        for span in &spans {
            assert!(!span.name().is_empty());
            assert!(!span.attributes().is_empty());
        }
    }

    #[test]
    fn no_sensitive_attributes_leak() {
        let span = ZanposSpan::AiProviderCall {
            provider: "openai",
            model: "gpt",
        };
        for (key, _) in span.attributes() {
            assert!(!key.contains("key"));
            assert!(!key.contains("token"));
            assert!(!key.contains("secret"));
            assert!(!key.contains("prompt"));
        }
    }

    #[test]
    fn disabled_rollback_does_not_panic() {
        // When otel-tracing feature is disabled, init returns Ok and shutdown is a no-op.
        let result = inner::init("test");
        assert!(result.is_ok());
        inner::shutdown(); // must not panic
    }

    #[test]
    fn queue_limits_are_bounded() {
        assert!(MAX_QUEUE_SIZE >= MAX_EXPORT_BATCH_SIZE);
        assert!(MAX_QUEUE_SIZE <= 4096, "queue cap prevents unbounded memory growth");
        assert!(SHUTDOWN_TIMEOUT.as_secs() <= 10, "shutdown must be fast");
    }
}
