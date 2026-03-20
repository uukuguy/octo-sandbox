//! Eval results overlay (Ctrl+E).
//!
//! Displays evaluation run summaries with task pass/fail counts
//! and score breakdowns. Reads from the eval output directory.

use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Wrap};

use crate::tui::app_state::TuiState;

/// Render the eval results overlay.
pub fn render(_state: &TuiState, frame: &mut Frame, area: Rect) {
    let inner = super::render_overlay_frame("Eval Results (Ctrl+E)", frame, area, Color::Magenta);

    // Two-column layout: run list | run details
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(inner);

    render_run_list(frame, columns[0]);
    render_run_details(frame, columns[1]);
}

/// Left column: list of eval runs.
fn render_run_list(frame: &mut Frame, area: Rect) {
    use ratatui::widgets::{Block, Borders};

    let block = Block::default()
        .title(" Runs ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Scan eval_output/runs/ for run directories
    let mut lines: Vec<Line> = Vec::new();

    let runs_dir = std::path::PathBuf::from("eval_output/runs");
    if runs_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&runs_dir) {
            let mut run_names: Vec<String> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().into_string().ok())
                .collect();
            run_names.sort();
            run_names.reverse(); // newest first

            for (i, name) in run_names.iter().take(inner.height as usize).enumerate() {
                let style = if i == 0 {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                lines.push(Line::from(Span::styled(format!("  {}", name), style)));
            }
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No eval runs found",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            "  Run: octo eval run --suite <name>",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(para, inner);
}

/// Right column: details for selected run.
fn render_run_details(frame: &mut Frame, area: Rect) {
    use ratatui::widgets::{Block, Borders};

    let block = Block::default()
        .title(" Details ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Read latest run's summary if available
    let mut lines: Vec<Line> = Vec::new();

    let runs_dir = std::path::PathBuf::from("eval_output/runs");
    let latest_summary = runs_dir
        .read_dir()
        .ok()
        .and_then(|entries| {
            let mut dirs: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .map(|e| e.path())
                .collect();
            dirs.sort();
            dirs.last().cloned()
        })
        .map(|dir| dir.join("summary.json"));

    if let Some(summary_path) = latest_summary {
        if summary_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&summary_path) {
                // Parse and display key metrics
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(suite) = json.get("suite").and_then(|v| v.as_str()) {
                        lines.push(Line::from(vec![
                            Span::styled("Suite: ", Style::default().fg(Color::DarkGray)),
                            Span::styled(suite.to_string(), Style::default().fg(Color::Cyan)),
                        ]));
                    }
                    if let Some(model) = json.get("model").and_then(|v| v.as_str()) {
                        lines.push(Line::from(vec![
                            Span::styled("Model: ", Style::default().fg(Color::DarkGray)),
                            Span::styled(model.to_string(), Style::default().fg(Color::Green)),
                        ]));
                    }
                    if let Some(total) = json.get("total_tasks").and_then(|v| v.as_u64()) {
                        let passed = json
                            .get("passed_tasks")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        lines.push(Line::from(""));
                        lines.push(Line::from(vec![
                            Span::styled("Passed: ", Style::default().fg(Color::DarkGray)),
                            Span::styled(
                                format!("{}/{}", passed, total),
                                Style::default().fg(if passed > total / 2 {
                                    Color::Green
                                } else {
                                    Color::Yellow
                                }),
                            ),
                        ]));
                        if total > 0 {
                            let rate = passed as f64 / total as f64 * 100.0;
                            lines.push(Line::from(vec![
                                Span::styled("Rate:   ", Style::default().fg(Color::DarkGray)),
                                Span::styled(
                                    format!("{:.1}%", rate),
                                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                                ),
                            ]));
                        }
                    }
                }
            }
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  Select a run to view details",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(para, inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_overlay_functions_exist() {
        let _ = render as fn(&TuiState, &mut Frame, Rect);
    }
}
