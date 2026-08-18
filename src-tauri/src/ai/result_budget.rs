#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetedToolResult {
    pub content: String,
    pub original_chars: usize,
    pub emitted_chars: usize,
    pub truncated: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ToolResultBudget {
    per_result_chars: usize,
    remaining_turn_chars: usize,
}

impl ToolResultBudget {
    pub fn new(per_result_chars: usize, per_turn_chars: usize) -> Self {
        Self {
            per_result_chars,
            remaining_turn_chars: per_turn_chars,
        }
    }

    pub fn apply(&mut self, tool_name: &str, content: String) -> BudgetedToolResult {
        let original_chars = content.chars().count();
        let allowed = self.per_result_chars.min(self.remaining_turn_chars);
        let emitted_chars = original_chars.min(allowed);
        self.remaining_turn_chars = self.remaining_turn_chars.saturating_sub(emitted_chars);

        if emitted_chars == original_chars {
            return BudgetedToolResult {
                content,
                original_chars,
                emitted_chars,
                truncated: false,
                reason: None,
            };
        }

        let reason = if self.remaining_turn_chars == 0 && allowed < self.per_result_chars {
            "turn_limit"
        } else {
            "per_result_limit"
        };
        let prefix: String = content.chars().take(emitted_chars).collect();
        let notice = serde_json::json!({
            "truncated": true,
            "reason": reason,
            "tool": tool_name,
            "original_chars": original_chars,
            "emitted_chars": emitted_chars,
            "next_action": "narrow the query, request an aggregate, or request the next page"
        });
        let content = if prefix.is_empty() {
            notice.to_string()
        } else {
            format!("{prefix}\n{notice}")
        };

        BudgetedToolResult {
            content,
            original_chars,
            emitted_chars,
            truncated: true,
            reason: Some(reason.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ToolResultBudget;

    #[test]
    fn leaves_small_results_unchanged() {
        let mut budget = ToolResultBudget::new(100, 200);
        let result = budget.apply("list_products", "three products".to_string());

        assert_eq!(result.content, "three products");
        assert_eq!(result.original_chars, 14);
        assert_eq!(result.emitted_chars, 14);
        assert!(!result.truncated);
        assert_eq!(result.reason, None);
    }

    #[test]
    fn truncates_per_result_on_unicode_character_boundaries() {
        let mut budget = ToolResultBudget::new(5, 100);
        let result = budget.apply("search_products", "أبجدهوز".to_string());

        assert!(result.truncated);
        assert_eq!(result.original_chars, 7);
        assert!(result.content.starts_with("أبجده"));
        assert!(result.content.contains("\"truncated\":true"));
        assert_eq!(result.reason.as_deref(), Some("per_result_limit"));
    }

    #[test]
    fn cumulative_limit_returns_a_continuation_signal_when_exhausted() {
        let mut budget = ToolResultBudget::new(20, 8);
        let first = budget.apply("first", "123456".to_string());
        let second = budget.apply("second", "abcdef".to_string());
        let exhausted = budget.apply("third", "more".to_string());

        assert!(!first.truncated);
        assert!(second.truncated);
        assert_eq!(second.reason.as_deref(), Some("turn_limit"));
        assert!(exhausted.truncated);
        assert_eq!(exhausted.emitted_chars, 0);
        assert!(exhausted.content.contains("narrow the query"));
    }
}
