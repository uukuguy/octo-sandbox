//! Notifier tool — P3-7.
//!
//! Sends notifications via multiple channels: desktop notification (notify-send/osascript),
//! webhook (Slack/Discord/generic), and file log. Useful for long-running autonomous tasks
//! or when the agent needs to alert the user asynchronously.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::process::Command;

use super::traits::Tool;
use octo_types::{ToolContext, ToolOutput, ToolSource};

pub struct NotifierTool;

#[async_trait]
impl Tool for NotifierTool {
    fn name(&self) -> &str {
        "notify"
    }

    fn description(&self) -> &str {
        "Send a notification to the user via one or more channels.\n\
         Channels:\n\
         - desktop: OS-level notification (macOS/Linux)\n\
         - webhook: POST to a URL (Slack, Discord, or custom)\n\
         - log: append to a notification log file\n\n\
         Parameters:\n\
         - message (required): notification text\n\
         - title: notification title (default: 'Octo Agent')\n\
         - channel: 'desktop', 'webhook', 'log', or 'all' (default: 'desktop')\n\
         - webhook_url: required if channel is 'webhook'\n\
         - urgency: 'low', 'normal', 'critical' (default: 'normal')\n\n\
         When to use: task completed, error needs attention, autonomous mode milestones.\n\
         When NOT to use: routine progress updates (use events instead)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Notification message body"
                },
                "title": {
                    "type": "string",
                    "description": "Notification title (default: 'Octo Agent')"
                },
                "channel": {
                    "type": "string",
                    "enum": ["desktop", "webhook", "log", "all"],
                    "description": "Notification channel (default: 'desktop')"
                },
                "webhook_url": {
                    "type": "string",
                    "description": "Webhook URL for Slack/Discord/custom (required if channel=webhook)"
                },
                "urgency": {
                    "type": "string",
                    "enum": ["low", "normal", "critical"],
                    "description": "Notification urgency level (default: 'normal')"
                }
            },
            "required": ["message"]
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let message = params
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: message"))?;
        let title = params
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Octo Agent");
        let channel = params
            .get("channel")
            .and_then(|v| v.as_str())
            .unwrap_or("desktop");
        let webhook_url = params.get("webhook_url").and_then(|v| v.as_str());
        let urgency = params
            .get("urgency")
            .and_then(|v| v.as_str())
            .unwrap_or("normal");

        let mut results: Vec<String> = Vec::new();

        let channels: Vec<&str> = if channel == "all" {
            vec!["desktop", "webhook", "log"]
        } else {
            vec![channel]
        };

        for ch in &channels {
            match *ch {
                "desktop" => {
                    let r = send_desktop(title, message, urgency);
                    results.push(format!("desktop: {}", if r.is_ok() { "sent" } else { "failed" }));
                }
                "webhook" => {
                    if let Some(url) = webhook_url {
                        match send_webhook(url, title, message).await {
                            Ok(()) => results.push("webhook: sent".to_string()),
                            Err(e) => results.push(format!("webhook: failed: {e}")),
                        }
                    } else {
                        results.push("webhook: skipped (no webhook_url)".to_string());
                    }
                }
                "log" => {
                    let r = append_log(&ctx.working_dir, title, message);
                    results.push(format!("log: {}", if r.is_ok() { "written" } else { "failed" }));
                }
                other => {
                    results.push(format!("{other}: unknown channel"));
                }
            }
        }

        Ok(ToolOutput::success(results.join("\n")))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn is_read_only(&self) -> bool {
        true // notifications don't modify the codebase
    }

    fn category(&self) -> &str {
        "notification"
    }
}

/// Send desktop notification using OS-specific tools.
fn send_desktop(title: &str, message: &str, urgency: &str) -> Result<()> {
    if cfg!(target_os = "macos") {
        // macOS: use osascript
        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            message.replace('\"', "\\\""),
            title.replace('\"', "\\\"")
        );
        Command::new("osascript")
            .args(["-e", &script])
            .output()?;
        Ok(())
    } else if cfg!(target_os = "linux") {
        // Linux: try notify-send
        let urg = match urgency {
            "critical" => "critical",
            "low" => "low",
            _ => "normal",
        };
        Command::new("notify-send")
            .args(["--urgency", urg, title, message])
            .output()?;
        Ok(())
    } else {
        anyhow::bail!("Desktop notifications not supported on this platform")
    }
}

/// Send webhook notification (Slack/Discord compatible JSON payload).
async fn send_webhook(url: &str, title: &str, message: &str) -> Result<()> {
    let payload = json!({
        "text": format!("*{title}*\n{message}"),
        "content": format!("**{title}**\n{message}"),
        "username": "Octo Agent"
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(url)
        .json(&payload)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?;

    if resp.status().is_success() {
        Ok(())
    } else {
        anyhow::bail!("Webhook returned {}", resp.status())
    }
}

/// Append notification to a log file in the working directory.
fn append_log(working_dir: &std::path::Path, title: &str, message: &str) -> Result<()> {
    use std::io::Write;

    let log_dir = working_dir.join(".octo");
    std::fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join("notifications.log");

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
    writeln!(file, "[{timestamp}] {title}: {message}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_ctx() -> ToolContext {
        ToolContext {
            sandbox_id: octo_types::SandboxId::default(),
            user_id: octo_types::UserId::from_string(octo_types::id::DEFAULT_USER_ID),
            working_dir: PathBuf::from("/tmp"),
            path_validator: None,
        }
    }

    #[test]
    fn test_notifier_metadata() {
        let tool = NotifierTool;
        assert_eq!(tool.name(), "notify");
        assert!(tool.is_read_only());
        assert_eq!(tool.category(), "notification");
    }

    #[tokio::test]
    async fn test_notify_log_channel() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ToolContext {
            sandbox_id: octo_types::SandboxId::default(),
            user_id: octo_types::UserId::from_string(octo_types::id::DEFAULT_USER_ID),
            working_dir: dir.path().to_path_buf(),
            path_validator: None,
        };
        let tool = NotifierTool;
        let result = tool
            .execute(
                json!({"message": "test notification", "channel": "log", "title": "Test"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("log: written"));

        // Verify log file exists
        let log_path = dir.path().join(".octo/notifications.log");
        assert!(log_path.exists());
        let content = std::fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("test notification"));
        assert!(content.contains("Test"));
    }

    #[tokio::test]
    async fn test_notify_webhook_no_url() {
        let tool = NotifierTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                json!({"message": "hello", "channel": "webhook"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.content.contains("skipped"));
    }

    #[tokio::test]
    async fn test_notify_desktop() {
        // Desktop notification — just verify it doesn't crash
        let tool = NotifierTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                json!({"message": "test", "channel": "desktop"}),
                &ctx,
            )
            .await
            .unwrap();
        // Either sent or failed, but shouldn't error
        assert!(!result.is_error);
    }

    #[test]
    fn test_append_log() {
        let dir = tempfile::tempdir().unwrap();
        append_log(dir.path(), "Title", "Message body").unwrap();

        let log_path = dir.path().join(".octo/notifications.log");
        let content = std::fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("Title: Message body"));
    }

    #[test]
    fn test_append_log_multiple() {
        let dir = tempfile::tempdir().unwrap();
        append_log(dir.path(), "First", "msg1").unwrap();
        append_log(dir.path(), "Second", "msg2").unwrap();

        let content = std::fs::read_to_string(dir.path().join(".octo/notifications.log")).unwrap();
        assert!(content.contains("msg1"));
        assert!(content.contains("msg2"));
        assert_eq!(content.lines().count(), 2);
    }
}
