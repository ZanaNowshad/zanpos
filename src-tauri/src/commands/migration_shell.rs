use crate::errors::{AppError, AppResult};

pub async fn exec_typed_command(command: String) -> AppResult<String> {
    let parsed = match parse_typed_command(&command) {
        Ok(parsed) => parsed,
        Err(AppError::Validation(message)) => return Ok(message),
        Err(err) => return Err(err),
    };
    tokio::task::spawn_blocking(move || match parsed {
        TypedCommand::Dir(path) => {
            let entries = std::fs::read_dir(&path)
                .map_err(|e| AppError::Internal(format!("Directory read failed: {e}")))?;
            let mut lines = Vec::new();
            for entry in entries.take(200) {
                let entry = entry
                    .map_err(|e| AppError::Internal(format!("Directory entry failed: {e}")))?;
                let meta = entry
                    .metadata()
                    .map_err(|e| AppError::Internal(format!("Metadata read failed: {e}")))?;
                let kind = if meta.is_dir() { "<DIR>" } else { "     " };
                lines.push(format!(
                    "{kind} {:>10} {}",
                    if meta.is_file() {
                        meta.len().to_string()
                    } else {
                        String::new()
                    },
                    entry.file_name().to_string_lossy()
                ));
            }
            Ok(if lines.is_empty() {
                "(empty directory)".into()
            } else {
                lines.join("\n")
            })
        }
        TypedCommand::ReadFile(path) => {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| AppError::Internal(format!("File read failed: {e}")))?;
            Ok(truncate_output(content))
        }
        TypedCommand::External { program, args } => {
            let output = std::process::Command::new(&program)
                .args(args)
                .output()
                .map_err(|e| AppError::Internal(format!("Typed command failed: {e}")))?;
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let mut result = truncate_output(stdout);
            if !stderr.is_empty() {
                result += &format!("\nSTDERR: {}", &stderr[..stderr.len().min(500)]);
            }
            if result.is_empty() {
                result = "(no output)".to_string();
            }
            Ok(result)
        }
    })
    .await
    .map_err(|e| AppError::Internal(format!("Typed command task failed: {e}")))?
}

enum TypedCommand {
    Dir(std::path::PathBuf),
    ReadFile(std::path::PathBuf),
    External { program: String, args: Vec<String> },
}

fn parse_typed_command(command: &str) -> AppResult<TypedCommand> {
    if command.contains(['&', '|', '>', '<', '^', ';', '`'])
        || command.contains("$(")
        || command.contains('\n')
        || command.contains('\r')
    {
        return Err(AppError::Validation(
            "Command rejected: shell metacharacters are not permitted.".into(),
        ));
    }

    let parts: Vec<&str> = command.split_whitespace().collect();
    let Some(verb) = parts.first().map(|p| p.to_ascii_lowercase()) else {
        return Err(AppError::Validation("Command is required".into()));
    };

    match verb.as_str() {
        "dir" => {
            let path = parts.get(1).copied().unwrap_or(".");
            safe_path_arg(path).map(TypedCommand::Dir)
        }
        "type" | "more" => {
            let path = parts
                .get(1)
                .ok_or_else(|| AppError::Validation("A file path is required".into()))?;
            safe_path_arg(path).map(TypedCommand::ReadFile)
        }
        "tasklist" => Ok(TypedCommand::External {
            program: "tasklist".into(),
            args: vec![],
        }),
        "where" => {
            let needle = parts
                .get(1)
                .ok_or_else(|| AppError::Validation("A command name is required".into()))?;
            if !needle
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
            {
                return Err(AppError::Validation("Invalid command name".into()));
            }
            Ok(TypedCommand::External {
                program: "where".into(),
                args: vec![needle.to_string()],
            })
        }
        "reg"
            if parts
                .get(1)
                .is_some_and(|v| v.eq_ignore_ascii_case("query")) =>
        {
            let key = parts
                .get(2)
                .ok_or_else(|| AppError::Validation("A registry key is required".into()))?;
            Ok(TypedCommand::External {
                program: "reg".into(),
                args: vec!["query".into(), key.to_string()],
            })
        }
        "wmic"
            if parts
                .get(1)
                .is_some_and(|v| v.eq_ignore_ascii_case("process")) =>
        {
            Ok(TypedCommand::External {
                program: "wmic".into(),
                args: vec![
                    "process".into(),
                    "get".into(),
                    "Name,ProcessId,ExecutablePath".into(),
                ],
            })
        }
        _ => Err(AppError::Validation(format!(
            "Command not permitted for safety reasons: '{command}'"
        ))),
    }
}

fn safe_path_arg(path: &str) -> AppResult<std::path::PathBuf> {
    if path.contains('*') || path.contains('?') {
        return Err(AppError::Validation("Wildcards are not permitted".into()));
    }
    if path != "." && !super::migration_commands::is_safe_read_path(path) {
        return Err(AppError::Validation(
            "Path is outside the approved read roots".into(),
        ));
    }
    Ok(std::path::PathBuf::from(path))
}

fn truncate_output(stdout: String) -> String {
    if stdout.len() > 6000 {
        format!("{}\n...(output truncated to 6000 chars)", &stdout[..6000])
    } else {
        stdout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn shell_rejects_free_form_echo_command() {
        let result = exec_typed_command("echo hello".to_string()).await.unwrap();
        assert!(
            result.starts_with("Command not permitted"),
            "free-form shell commands must be rejected, got: {result}"
        );
    }

    #[tokio::test]
    async fn shell_rejects_powershell_free_form_command() {
        let result = exec_typed_command("powershell -NoProfile Get-ChildItem".to_string())
            .await
            .unwrap();
        assert!(
            result.starts_with("Command not permitted"),
            "PowerShell command strings must be replaced by typed commands, got: {result}"
        );
    }
}
