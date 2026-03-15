//! Versioned evaluation run storage — manages run directories on disk.
//!
//! Each run is stored in `{base_dir}/{run_id}/` with:
//! - `manifest.json` — run metadata
//! - `report.json`   — detailed report (optional)
//! - `comparison.json` — comparison data (optional)
//! - `traces/trace_{task_id}.json` — per-task traces

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::benchmark::FailureSummary;
use crate::recorder::EvalTrace;
use crate::reporter::DetailedReport;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Manifest stored alongside every run — lightweight metadata for listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunManifest {
    pub run_id: String,
    pub tag: Option<String>,
    pub timestamp: String,
    /// "run" | "compare" | "benchmark"
    pub command: String,
    pub suite: String,
    pub models: Vec<String>,
    pub git_commit: String,
    pub git_branch: String,
    pub task_count: usize,
    pub passed: usize,
    pub pass_rate: f64,
    pub avg_score: f64,
    pub duration_ms: u64,
    pub total_tokens: u64,
    pub estimated_cost: f64,
    pub eval_config_hash: String,
    pub failure_summary: FailureSummary,
}

/// Full run data — manifest plus optional artefacts.
pub struct RunData {
    pub manifest: RunManifest,
    pub report: Option<DetailedReport>,
    /// Stored as opaque JSON because `ComparisonReport` is not Serialize.
    pub comparison: Option<serde_json::Value>,
    pub traces: Vec<EvalTrace>,
}

/// Filter parameters for `list_runs`.
pub struct RunFilter {
    pub suite: Option<String>,
    pub since: Option<String>,
    pub limit: usize,
    pub tag: Option<String>,
}

impl Default for RunFilter {
    fn default() -> Self {
        Self {
            suite: None,
            since: None,
            limit: 50,
            tag: None,
        }
    }
}

// ---------------------------------------------------------------------------
// RunStore
// ---------------------------------------------------------------------------

/// Manages versioned evaluation runs on disk.
pub struct RunStore {
    base_dir: PathBuf,
}

impl RunStore {
    /// Create a new `RunStore`, creating `base_dir` if it does not exist.
    pub fn new(base_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&base_dir)
            .with_context(|| format!("Failed to create run store at {}", base_dir.display()))?;
        Ok(Self { base_dir })
    }

    /// Generate the next run ID in `YYYY-MM-DD-NNN` format.
    ///
    /// Scans existing directories matching today's date prefix and increments
    /// the 3-digit sequence number.
    pub fn next_run_id(&self) -> String {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let prefix = format!("{}-", today);

        let mut max_seq: u32 = 0;
        if let Ok(entries) = std::fs::read_dir(&self.base_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if let Some(suffix) = name.strip_prefix(&prefix) {
                    if let Ok(seq) = suffix.parse::<u32>() {
                        max_seq = max_seq.max(seq);
                    }
                }
            }
        }

        format!("{}-{:03}", today, max_seq + 1)
    }

    /// Persist a complete run to disk.
    ///
    /// Creates `{base_dir}/{run_id}/` and writes manifest, report,
    /// comparison, and traces.
    pub fn save_run(&self, run: &RunData) -> Result<PathBuf> {
        let run_dir = self.base_dir.join(&run.manifest.run_id);
        std::fs::create_dir_all(&run_dir)?;

        // manifest.json
        let manifest_path = run_dir.join("manifest.json");
        let json = serde_json::to_string_pretty(&run.manifest)?;
        std::fs::write(&manifest_path, json)?;

        // report.json (optional)
        if let Some(ref report) = run.report {
            let report_path = run_dir.join("report.json");
            let json = serde_json::to_string_pretty(report)?;
            std::fs::write(&report_path, json)?;
        }

        // comparison.json (optional)
        if let Some(ref comparison) = run.comparison {
            let comparison_path = run_dir.join("comparison.json");
            let json = serde_json::to_string_pretty(comparison)?;
            std::fs::write(&comparison_path, json)?;
        }

        // traces/
        if !run.traces.is_empty() {
            let traces_dir = run_dir.join("traces");
            std::fs::create_dir_all(&traces_dir)?;
            for trace in &run.traces {
                let filename = format!("trace_{}.json", trace.task_id.replace('/', "_"));
                let trace_path = traces_dir.join(&filename);
                let json = serde_json::to_string_pretty(trace)?;
                std::fs::write(&trace_path, json)?;
            }
        }

        tracing::info!(run_id = %run.manifest.run_id, dir = %run_dir.display(), "Saved evaluation run");
        Ok(run_dir)
    }

    /// List runs matching the given filter, sorted by run_id descending.
    pub fn list_runs(&self, filter: &RunFilter) -> Result<Vec<RunManifest>> {
        let mut manifests = Vec::new();

        let entries = std::fs::read_dir(&self.base_dir)
            .with_context(|| format!("Cannot read run store at {}", self.base_dir.display()))?;

        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let manifest_path = entry.path().join("manifest.json");
            if !manifest_path.exists() {
                continue;
            }
            match self.load_manifest_from_path(&manifest_path) {
                Ok(m) => manifests.push(m),
                Err(e) => {
                    tracing::warn!(path = %manifest_path.display(), err = %e, "Skipping corrupt manifest");
                }
            }
        }

        // Apply filters
        if let Some(ref suite) = filter.suite {
            manifests.retain(|m| &m.suite == suite);
        }
        if let Some(ref tag) = filter.tag {
            manifests.retain(|m| m.tag.as_deref() == Some(tag.as_str()));
        }
        if let Some(ref since) = filter.since {
            manifests.retain(|m| m.run_id.as_str() >= since.as_str());
        }

        // Sort descending by run_id
        manifests.sort_by(|a, b| b.run_id.cmp(&a.run_id));

        // Apply limit
        manifests.truncate(filter.limit);

        Ok(manifests)
    }

    /// Load a complete run from disk.
    pub fn load_run(&self, run_id: &str) -> Result<RunData> {
        let run_dir = self.base_dir.join(run_id);
        if !run_dir.is_dir() {
            bail!("Run directory not found: {}", run_dir.display());
        }

        let manifest = self.load_manifest(run_id)?;

        // report.json
        let report = {
            let path = run_dir.join("report.json");
            if path.exists() {
                let data = std::fs::read_to_string(&path)?;
                Some(serde_json::from_str::<DetailedReport>(&data)?)
            } else {
                None
            }
        };

        // comparison.json
        let comparison = {
            let path = run_dir.join("comparison.json");
            if path.exists() {
                let data = std::fs::read_to_string(&path)?;
                Some(serde_json::from_str::<serde_json::Value>(&data)?)
            } else {
                None
            }
        };

        // traces/
        let mut traces = Vec::new();
        let traces_dir = run_dir.join("traces");
        if traces_dir.is_dir() {
            let mut trace_files: Vec<_> = std::fs::read_dir(&traces_dir)?
                .flatten()
                .filter(|e| {
                    e.path()
                        .extension()
                        .map_or(false, |ext| ext == "json")
                })
                .collect();
            trace_files.sort_by_key(|e| e.file_name());
            for entry in trace_files {
                let data = std::fs::read_to_string(entry.path())?;
                let trace: EvalTrace = serde_json::from_str(&data)?;
                traces.push(trace);
            }
        }

        Ok(RunData {
            manifest,
            report,
            comparison,
            traces,
        })
    }

    /// Load only the manifest for a given run.
    pub fn load_manifest(&self, run_id: &str) -> Result<RunManifest> {
        let manifest_path = self.base_dir.join(run_id).join("manifest.json");
        self.load_manifest_from_path(&manifest_path)
    }

    /// Create or update a `latest` symlink pointing to the given run.
    ///
    /// On non-Unix platforms this is a no-op (logs a warning).
    pub fn update_latest_link(&self, run_id: &str) -> Result<()> {
        #[cfg(unix)]
        {
            let link_path = self
                .base_dir
                .parent()
                .unwrap_or(&self.base_dir)
                .join("latest");
            let target = self.base_dir.join(run_id);

            // Remove existing symlink/file if present
            if link_path.exists() || link_path.symlink_metadata().is_ok() {
                let _ = std::fs::remove_file(&link_path);
            }

            std::os::unix::fs::symlink(&target, &link_path).with_context(|| {
                format!(
                    "Failed to create symlink {} -> {}",
                    link_path.display(),
                    target.display()
                )
            })?;

            tracing::info!(link = %link_path.display(), target = %target.display(), "Updated latest symlink");
        }

        #[cfg(not(unix))]
        {
            tracing::warn!("Symlink creation not supported on this platform, skipping latest link for run {}", run_id);
        }

        Ok(())
    }

    /// Tag a run by updating its manifest.
    pub fn tag_run(&self, run_id: &str, tag: &str) -> Result<()> {
        let manifest_path = self.base_dir.join(run_id).join("manifest.json");
        let mut manifest = self.load_manifest_from_path(&manifest_path)?;
        manifest.tag = Some(tag.to_string());
        let json = serde_json::to_string_pretty(&manifest)?;
        std::fs::write(&manifest_path, json)?;
        tracing::info!(run_id = %run_id, tag = %tag, "Tagged run");
        Ok(())
    }

    // -- helpers --

    fn load_manifest_from_path(&self, path: &Path) -> Result<RunManifest> {
        let data = std::fs::read_to_string(path)
            .with_context(|| format!("Cannot read manifest at {}", path.display()))?;
        let manifest: RunManifest = serde_json::from_str(&data)
            .with_context(|| format!("Cannot parse manifest at {}", path.display()))?;
        Ok(manifest)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn sample_manifest(run_id: &str, suite: &str) -> RunManifest {
        RunManifest {
            run_id: run_id.to_string(),
            tag: None,
            timestamp: "2026-03-15T10:00:00+08:00".to_string(),
            command: "run".to_string(),
            suite: suite.to_string(),
            models: vec!["test-model".to_string()],
            git_commit: "abc1234".to_string(),
            git_branch: "main".to_string(),
            task_count: 10,
            passed: 8,
            pass_rate: 0.8,
            avg_score: 0.75,
            duration_ms: 5000,
            total_tokens: 1000,
            estimated_cost: 0.05,
            eval_config_hash: "hash123".to_string(),
            failure_summary: FailureSummary::default(),
        }
    }

    fn sample_trace(task_id: &str) -> EvalTrace {
        EvalTrace {
            task_id: task_id.to_string(),
            timestamp: "2026-03-15T10:00:00+08:00".to_string(),
            interactions: vec![],
            timeline: vec![],
            output: crate::task::AgentOutput {
                messages: vec![],
                tool_calls: vec![],
                rounds: 1,
                input_tokens: 100,
                output_tokens: 50,
                duration_ms: 1000,
                stop_reason: "end_turn".to_string(),
            },
            score: crate::score::EvalScore {
                passed: true,
                score: 1.0,
                details: crate::score::ScoreDetails::ExactMatch {
                    expected: "expected".to_string(),
                    actual: "expected".to_string(),
                },
                dimensions: HashMap::new(),
                failure_class: None,
            },
        }
    }

    fn sample_report() -> DetailedReport {
        DetailedReport {
            summary: crate::reporter::ReportSummary {
                total: 10,
                passed: 8,
                failed: 2,
                pass_rate: 0.8,
                avg_score: 0.75,
            },
            by_category: HashMap::new(),
            by_difficulty: HashMap::new(),
            latency: crate::reporter::LatencyStats::default(),
            token_usage: crate::reporter::TokenUsageStats::default(),
            task_results: vec![],
        }
    }

    #[test]
    fn test_next_run_id_empty_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = RunStore::new(tmp.path().join("runs")).unwrap();
        let id = store.next_run_id();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        assert_eq!(id, format!("{}-001", today));
    }

    #[test]
    fn test_next_run_id_sequential() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path().join("runs");
        let store = RunStore::new(base.clone()).unwrap();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        // Create two existing run dirs
        std::fs::create_dir_all(base.join(format!("{}-001", today))).unwrap();
        std::fs::create_dir_all(base.join(format!("{}-003", today))).unwrap();

        let id = store.next_run_id();
        assert_eq!(id, format!("{}-004", today));
    }

    #[test]
    fn test_save_and_load_manifest() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = RunStore::new(tmp.path().join("runs")).unwrap();

        let manifest = sample_manifest("2026-03-15-001", "tool_call");
        let run = RunData {
            manifest: manifest.clone(),
            report: None,
            comparison: None,
            traces: vec![],
        };

        store.save_run(&run).unwrap();

        let loaded = store.load_manifest("2026-03-15-001").unwrap();
        assert_eq!(loaded.run_id, "2026-03-15-001");
        assert_eq!(loaded.suite, "tool_call");
        assert_eq!(loaded.passed, 8);
        assert!(loaded.tag.is_none());
    }

    #[test]
    fn test_save_and_load_full_run() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = RunStore::new(tmp.path().join("runs")).unwrap();

        let run = RunData {
            manifest: sample_manifest("2026-03-15-002", "security"),
            report: Some(sample_report()),
            comparison: Some(serde_json::json!({"summary": "test comparison"})),
            traces: vec![
                sample_trace("task-01"),
                sample_trace("task-02"),
            ],
        };

        let run_dir = store.save_run(&run).unwrap();
        assert!(run_dir.join("manifest.json").exists());
        assert!(run_dir.join("report.json").exists());
        assert!(run_dir.join("comparison.json").exists());
        assert!(run_dir.join("traces/trace_task-01.json").exists());
        assert!(run_dir.join("traces/trace_task-02.json").exists());

        let loaded = store.load_run("2026-03-15-002").unwrap();
        assert_eq!(loaded.manifest.suite, "security");
        assert!(loaded.report.is_some());
        assert!(loaded.comparison.is_some());
        assert_eq!(loaded.traces.len(), 2);
        assert_eq!(loaded.traces[0].task_id, "task-01");
    }

    #[test]
    fn test_list_runs_with_filter() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = RunStore::new(tmp.path().join("runs")).unwrap();

        // Save runs with different suites
        for (id, suite) in [
            ("2026-03-14-001", "tool_call"),
            ("2026-03-14-002", "security"),
            ("2026-03-15-001", "tool_call"),
        ] {
            let run = RunData {
                manifest: sample_manifest(id, suite),
                report: None,
                comparison: None,
                traces: vec![],
            };
            store.save_run(&run).unwrap();
        }

        // Filter by suite
        let filter = RunFilter {
            suite: Some("tool_call".to_string()),
            ..Default::default()
        };
        let results = store.list_runs(&filter).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|m| m.suite == "tool_call"));
    }

    #[test]
    fn test_tag_run() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = RunStore::new(tmp.path().join("runs")).unwrap();

        let run = RunData {
            manifest: sample_manifest("2026-03-15-001", "tool_call"),
            report: None,
            comparison: None,
            traces: vec![],
        };
        store.save_run(&run).unwrap();

        // Tag the run
        store.tag_run("2026-03-15-001", "baseline-v1").unwrap();

        let loaded = store.load_manifest("2026-03-15-001").unwrap();
        assert_eq!(loaded.tag.as_deref(), Some("baseline-v1"));

        // Filter by tag
        let filter = RunFilter {
            tag: Some("baseline-v1".to_string()),
            ..Default::default()
        };
        let results = store.list_runs(&filter).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].run_id, "2026-03-15-001");
    }

    #[test]
    fn test_list_runs_sorted_descending() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = RunStore::new(tmp.path().join("runs")).unwrap();

        let ids = [
            "2026-03-13-001",
            "2026-03-15-002",
            "2026-03-14-001",
            "2026-03-15-001",
        ];
        for id in &ids {
            let run = RunData {
                manifest: sample_manifest(id, "tool_call"),
                report: None,
                comparison: None,
                traces: vec![],
            };
            store.save_run(&run).unwrap();
        }

        let results = store.list_runs(&RunFilter::default()).unwrap();
        assert_eq!(results.len(), 4);
        assert_eq!(results[0].run_id, "2026-03-15-002");
        assert_eq!(results[1].run_id, "2026-03-15-001");
        assert_eq!(results[2].run_id, "2026-03-14-001");
        assert_eq!(results[3].run_id, "2026-03-13-001");
    }

    #[cfg(unix)]
    #[test]
    fn test_update_latest_link() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = RunStore::new(tmp.path().join("runs")).unwrap();

        let run = RunData {
            manifest: sample_manifest("2026-03-15-001", "tool_call"),
            report: None,
            comparison: None,
            traces: vec![],
        };
        store.save_run(&run).unwrap();

        store.update_latest_link("2026-03-15-001").unwrap();

        let link_path = tmp.path().join("latest");
        assert!(link_path.exists());
        let target = std::fs::read_link(&link_path).unwrap();
        assert!(target.ends_with("runs/2026-03-15-001"));
    }
}
