//! Streaming render engine — renders AgentEvent stream to the terminal

use anyhow::Result;
use owo_colors::OwoColorize;
use std::io::Write;
use tokio::sync::broadcast;

use octo_engine::agent::AgentEvent;

use crate::output::{OutputConfig, OutputFormat};

/// Render a streaming response from an agent event channel.
///
/// Dispatches to text, json, or stream-json renderers based on the output config.
pub async fn render_streaming_response(
    rx: &mut broadcast::Receiver<AgentEvent>,
    config: &OutputConfig,
) -> Result<()> {
    match config.format {
        OutputFormat::Text => render_text_stream(rx, config).await,
        OutputFormat::Json => render_json_stream(rx).await,
        OutputFormat::StreamJson => render_stream_json(rx).await,
    }
}

/// Text renderer — human-readable terminal output with colors and spinners.
async fn render_text_stream(
    rx: &mut broadcast::Receiver<AgentEvent>,
    _config: &OutputConfig,
) -> Result<()> {
    let mut active_spinner: Option<indicatif::ProgressBar> = None;

    loop {
        match rx.recv().await {
            Ok(event) => match event {
                AgentEvent::TextDelta { text } => {
                    print!("{}", text);
                    std::io::stdout().flush()?;
                }
                AgentEvent::TextComplete { .. } => {
                    // Final text already delivered via TextDelta; just ensure newline
                }
                AgentEvent::ThinkingDelta { text } => {
                    print!("{}", text.dimmed());
                    std::io::stdout().flush()?;
                }
                AgentEvent::ThinkingComplete { .. } => {
                    // Thinking block done
                }
                AgentEvent::ToolStart {
                    tool_name, input, ..
                } => {
                    let preview = truncate_json(&input, 60);
                    let spinner = super::spinner::create_tool_spinner(&tool_name, &preview);
                    active_spinner = Some(spinner);
                }
                AgentEvent::ToolResult {
                    output, success, ..
                } => {
                    if let Some(spinner) = active_spinner.take() {
                        spinner.finish_and_clear();
                    }
                    if success {
                        let preview = truncate(&output, 120);
                        eprintln!("  {} {}", "✔".green(), preview.dimmed());
                    } else {
                        let preview = truncate(&output, 120);
                        eprintln!("  {} {}", "✘".red(), preview.red());
                    }
                }
                AgentEvent::Error { message } => {
                    eprintln!("{}", format!("Error: {}", message).red().bold());
                }
                AgentEvent::Done => {
                    println!();
                    break;
                }
                AgentEvent::Completed(result) => {
                    println!();
                    eprintln!(
                        "{}",
                        format!(
                            "[{} rounds, {} tool calls, stop: {:?}]",
                            result.rounds, result.tool_calls, result.stop_reason
                        )
                        .dimmed()
                    );
                    break;
                }
                AgentEvent::IterationStart { round } => {
                    tracing::debug!("iteration start: round {}", round);
                }
                AgentEvent::IterationEnd { round } => {
                    tracing::debug!("iteration end: round {}", round);
                }
                // Silently ignore remaining events
                _ => {}
            },
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("streaming receiver lagged, skipped {} events", n);
            }
            Err(broadcast::error::RecvError::Closed) => {
                break;
            }
        }
    }

    Ok(())
}

/// JSON renderer — each event as a single JSON line to stdout.
async fn render_json_stream(rx: &mut broadcast::Receiver<AgentEvent>) -> Result<()> {
    loop {
        match rx.recv().await {
            Ok(event) => {
                let line = serde_json::to_string(&event)?;
                println!("{}", line);

                if matches!(event, AgentEvent::Done | AgentEvent::Completed(_)) {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("json stream receiver lagged, skipped {} events", n);
            }
            Err(broadcast::error::RecvError::Closed) => {
                break;
            }
        }
    }

    Ok(())
}

/// Stream-JSON renderer — each event as a JSON line (same format, distinct mode).
async fn render_stream_json(rx: &mut broadcast::Receiver<AgentEvent>) -> Result<()> {
    loop {
        match rx.recv().await {
            Ok(event) => {
                let line = serde_json::to_string(&event)?;
                println!("{}", line);

                if matches!(event, AgentEvent::Done | AgentEvent::Completed(_)) {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("stream-json receiver lagged, skipped {} events", n);
            }
            Err(broadcast::error::RecvError::Closed) => {
                break;
            }
        }
    }

    Ok(())
}

/// Truncate a JSON value to a compact string of at most `max_len` characters.
fn truncate_json(value: &serde_json::Value, max_len: usize) -> String {
    let s = value.to_string();
    truncate(&s, max_len)
}

/// Truncate a string to `max_len` characters, appending "..." if truncated.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long() {
        let result = truncate("hello world, this is long", 10);
        assert_eq!(result, "hello w...");
        assert!(result.len() <= 10);
    }

    #[test]
    fn test_truncate_json_object() {
        let val = serde_json::json!({"key": "value"});
        let result = truncate_json(&val, 50);
        assert!(result.len() <= 50);
    }
}
