/// ZANPOS OpenTelemetry span taxonomy.
///
/// Each enum variant maps to a named span in the OTel trace.
/// Attributes are deny-by-default; only explicitly allowlisted keys pass through.

pub enum ZanposSpan {
    TauriCommand { command: &'static str },
    DbTransaction { operation: &'static str },
    SyncBatch { table_count: usize },
    PrinterOperation { kind: &'static str },
    AiProviderCall { provider: &'static str, model: &'static str },
    AiToolExecution { tool: &'static str },
    BackupOperation { phase: &'static str },
    WhatsAppQueue { action: &'static str },
    StorefrontPublish { stage: &'static str },
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
            ZanposSpan::DbTransaction { operation } => vec![("db.operation", operation.to_string())],
            ZanposSpan::SyncBatch { table_count } => vec![("sync.table_count", table_count.to_string())],
            ZanposSpan::PrinterOperation { kind } => vec![("printer.kind", kind.to_string())],
            ZanposSpan::AiProviderCall { provider, model } => vec![
                ("ai.provider", provider.to_string()),
                ("ai.model", model.to_string()),
            ],
            ZanposSpan::AiToolExecution { tool } => vec![("ai.tool", tool.to_string())],
            ZanposSpan::BackupOperation { phase } => vec![("backup.phase", phase.to_string())],
            ZanposSpan::WhatsAppQueue { action } => vec![("whatsapp.action", action.to_string())],
            ZanposSpan::StorefrontPublish { stage } => vec![("storefront.stage", stage.to_string())],
        }
    }
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
            ZanposSpan::AiProviderCall { provider: "x", model: "y" },
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
        let span = ZanposSpan::AiProviderCall { provider: "openai", model: "gpt" };
        for (key, _) in span.attributes() {
            assert!(!key.contains("key"));
            assert!(!key.contains("token"));
            assert!(!key.contains("secret"));
            assert!(!key.contains("prompt"));
        }
    }
}
