//! GAIA benchmark adapter — multi-step reasoning + multi-tool evaluation.
//!
//! GAIA (General AI Assistants) evaluates multi-step reasoning with exact-match scoring.
//! Level 1: single-step, Level 2: multi-step + multi-tool, Level 3: complex long-chain.

use std::path::PathBuf;

use std::collections::HashMap;

use serde::Deserialize;

use crate::benchmarks::{ExternalBenchmark, MetricDefinition};
use crate::score::{EvalScore, ScoreDetails};
use crate::task::{AgentOutput, Difficulty, EvalTask, TaskMetadata};

/// A single GAIA evaluation record parsed from JSONL
#[derive(Debug, Clone, Deserialize)]
pub struct GaiaRecord {
    pub task_id: String,
    pub question: String,
    pub final_answer: String,
    pub level: u32,
    #[serde(default)]
    pub annotator_metadata: Option<GaiaAnnotation>,
    /// Attached file name (e.g. "abc123.xlsx") — present in 38/165 validation tasks
    #[serde(default)]
    pub file_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GaiaAnnotation {
    #[serde(default)]
    pub steps: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub num_steps: u32,
}

/// EvalTask implementation for a single GAIA task
pub struct GaiaTask {
    record: GaiaRecord,
    /// Full prompt including file path hint (built once at construction)
    prompt: String,
}

impl GaiaTask {
    /// Answer format instruction appended to every GAIA prompt.
    const ANSWER_FORMAT_INSTRUCTION: &'static str =
        "\n\nIMPORTANT: You MUST end your response with exactly one line in this format:\nFINAL ANSWER: <your answer>\nThe answer should be concise — a single word, number, or short phrase. Do NOT include explanations in the FINAL ANSWER line.";

    pub fn new(record: GaiaRecord) -> Self {
        // Build prompt: reference attachment by relative filename only.
        // The runner copies the file into the task working directory before execution.
        let mut prompt = if let Some(ref fname) = record.file_name {
            if !fname.is_empty() {
                format!(
                    "{}\n\nNote: An attached file is available at `{}`. Use the file_read or bash tool to read its contents.",
                    record.question, fname
                )
            } else {
                record.question.clone()
            }
        } else {
            record.question.clone()
        };
        prompt.push_str(Self::ANSWER_FORMAT_INSTRUCTION);
        Self { record, prompt }
    }

    fn classify_difficulty(level: u32) -> Difficulty {
        match level {
            1 => Difficulty::Easy,
            2 => Difficulty::Medium,
            _ => Difficulty::Hard,
        }
    }

    /// Resolve the source path for a GAIA attachment file in the dataset directory.
    fn source_file_path(file_name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("datasets/gaia_files")
            .join(file_name)
    }
}

/// Normalize answer for comparison.
/// GAIA standard: trim, lowercase, strip trailing punctuation and quotes.
fn normalize_answer(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .trim_matches(|c: char| c == '"' || c == '\'' || c == '`')
        .trim_end_matches(|c: char| c == '.' || c == ',' || c == ';')
        .trim()
        .to_string()
}

/// Extract the final answer from the agent's response text.
///
/// Strategy (in priority order):
/// 1. Look for "FINAL ANSWER: xxx" pattern (case-insensitive)
/// 2. Look for "The answer is xxx" pattern
/// 3. Fall back to the last non-empty line
fn extract_answer(text: &str) -> String {
    let lower = text.to_lowercase();

    // Strategy 1: Find "FINAL ANSWER:" anywhere in text (last occurrence)
    if let Some(idx) = lower.rfind("final answer:") {
        let after = &text[idx + "final answer:".len()..];
        let answer = after.lines().next().unwrap_or("").trim()
            .trim_start_matches('*').trim_end_matches('*').trim();
        if !answer.is_empty() {
            return answer.to_string();
        }
    }

    // Strategy 1b: "FINAL ANSWER :" with space before colon
    if let Some(idx) = lower.rfind("final answer :") {
        let after = &text[idx + "final answer :".len()..];
        let answer = after.lines().next().unwrap_or("").trim();
        if !answer.is_empty() {
            return answer.to_string();
        }
    }

    // Strategy 2: "The answer is ..." pattern (last occurrence)
    if let Some(idx) = lower.rfind("the answer is ") {
        let after = &text[idx + "the answer is ".len()..];
        let answer = after.lines().next().unwrap_or("").trim()
            .trim_end_matches(|c: char| c == '.' || c == ',' || c == ';')
            .trim_start_matches("**").trim_end_matches("**").trim();
        if !answer.is_empty() {
            return answer.to_string();
        }
    }

    // Strategy 3: last non-empty line (common when agent just outputs the answer)
    for line in text.lines().rev() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    text.trim().to_string()
}

/// Check if two normalized answers match using multiple strategies.
///
/// Returns (matched, score):
/// - Exact match after normalization → (true, 1.0)
/// - Expected is contained in actual (for short expected answers) → (true, 0.8)
/// - Actual is contained in expected → (true, 0.8)
fn fuzzy_match(normalized_expected: &str, normalized_actual: &str) -> (bool, f64) {
    // Exact match
    if normalized_actual == normalized_expected {
        return (true, 1.0);
    }

    // Contains match: expected answer appears in the actual response
    // Only for short expected answers (< 100 chars) to avoid false positives
    if normalized_expected.len() < 100 && normalized_actual.contains(normalized_expected) {
        return (true, 0.8);
    }

    // Reverse contains: actual answer appears in expected (for when agent gives partial answer)
    if normalized_actual.len() < 100
        && !normalized_actual.is_empty()
        && normalized_expected.contains(normalized_actual)
    {
        return (true, 0.8);
    }

    // Numeric comparison: handle formatting differences like "1,000" vs "1000"
    if let (Some(n1), Some(n2)) = (parse_number(normalized_expected), parse_number(normalized_actual)) {
        if (n1 - n2).abs() < f64::EPSILON * 100.0 {
            return (true, 1.0);
        }
    }

    (false, 0.0)
}

/// Try to parse a string as a number, stripping commas and currency symbols.
fn parse_number(s: &str) -> Option<f64> {
    let cleaned: String = s
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    cleaned.parse::<f64>().ok()
}

impl EvalTask for GaiaTask {
    fn id(&self) -> &str {
        &self.record.task_id
    }

    fn prompt(&self) -> &str {
        &self.prompt
    }

    fn available_tools(&self) -> Option<Vec<octo_types::tool::ToolSpec>> {
        // GAIA tasks require multi-tool access: web search, file reading, bash execution.
        // Return specs to document available capabilities; tools are provided via default_tools().
        // Level 1: basic reasoning (no tools needed but available)
        // Level 2+: multi-tool required
        Some(vec![
            octo_types::tool::ToolSpec {
                name: "bash".to_string(),
                description: "Execute shell commands for computation, data processing, and system operations".to_string(),
                input_schema: serde_json::json!({"type": "object", "properties": {"command": {"type": "string"}}, "required": ["command"]}),
            },
            octo_types::tool::ToolSpec {
                name: "web_search".to_string(),
                description: "Search the web for current information, facts, and references".to_string(),
                input_schema: serde_json::json!({"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}),
            },
            octo_types::tool::ToolSpec {
                name: "file_read".to_string(),
                description: "Read file contents from the local filesystem".to_string(),
                input_schema: serde_json::json!({"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}),
            },
        ])
    }

    fn score(&self, output: &AgentOutput) -> EvalScore {
        let raw_response = output
            .messages
            .last()
            .map(|m| m.text_content())
            .unwrap_or_default();

        // Extract the answer from the agent's response (not the full text)
        let extracted = extract_answer(&raw_response);
        let normalized_expected = normalize_answer(&self.record.final_answer);
        let normalized_actual = normalize_answer(&extracted);

        let (passed, score) = fuzzy_match(&normalized_expected, &normalized_actual);
        EvalScore {
            passed,
            score,
            details: ScoreDetails::GaiaMatch {
                expected: self.record.final_answer.clone(),
                actual: extracted,
                level: self.record.level,
            },
            dimensions: HashMap::new(),
            failure_class: None,
        }
    }

    fn scoring_data(&self) -> serde_json::Value {
        serde_json::json!({
            "benchmark": "gaia",
            "final_answer": self.record.final_answer,
            "level": self.record.level,
        })
    }

    fn metadata(&self) -> TaskMetadata {
        TaskMetadata {
            category: format!("gaia-L{}", self.record.level),
            difficulty: Self::classify_difficulty(self.record.level),
            expected_steps: self
                .record
                .annotator_metadata
                .as_ref()
                .map(|m| m.num_steps),
            tags: vec!["external".into(), "gaia".into()],
        }
    }

    fn attached_files(&self) -> Vec<(std::path::PathBuf, String)> {
        match &self.record.file_name {
            Some(fname) if !fname.is_empty() => {
                vec![(Self::source_file_path(fname), fname.clone())]
            }
            _ => Vec::new(),
        }
    }
}

/// Filter configuration for GAIA tasks.
///
/// Excludes tasks that require capabilities the agent doesn't have
/// (e.g. image OCR, audio transcription, video understanding).
#[derive(Debug, Clone, Default)]
pub struct GaiaFilter {
    /// File extensions to exclude (e.g. ["png", "jpg", "mp3"])
    pub exclude_file_extensions: Vec<String>,
    /// Patterns in question text to exclude (e.g. ["youtube.com"])
    pub exclude_question_patterns: Vec<String>,
}

impl GaiaFilter {
    /// Default filter: exclude image/audio/pptx attachments and YouTube URLs.
    pub fn default_capability_filter() -> Self {
        Self {
            exclude_file_extensions: vec![
                "png".into(), "jpg".into(), "jpeg".into(), "gif".into(),
                "mp3".into(), "pptx".into(),
            ],
            exclude_question_patterns: vec!["youtube.com".into()],
        }
    }

    /// Returns true if the record should be excluded.
    fn should_exclude(&self, record: &GaiaRecord) -> bool {
        // Check file extension
        if let Some(ref fname) = record.file_name {
            if !fname.is_empty() {
                if let Some(ext) = fname.rsplit('.').next() {
                    let ext_lower = ext.to_lowercase();
                    if self.exclude_file_extensions.iter().any(|e| e == &ext_lower) {
                        return true;
                    }
                }
            }
        }
        // Check question patterns
        let q_lower = record.question.to_lowercase();
        for pattern in &self.exclude_question_patterns {
            if q_lower.contains(&pattern.to_lowercase()) {
                return true;
            }
        }
        false
    }
}

/// GAIA benchmark adapter
pub struct GaiaBenchmark {
    dataset_path: Option<PathBuf>,
    filter: Option<GaiaFilter>,
}

impl GaiaBenchmark {
    pub fn new() -> Self {
        Self {
            dataset_path: None,
            filter: None,
        }
    }

    pub fn with_dataset(path: PathBuf) -> Self {
        Self {
            dataset_path: Some(path),
            filter: None,
        }
    }

    /// Create with default capability filter (excludes image/audio/video tasks).
    pub fn with_default_filter() -> Self {
        Self {
            dataset_path: None,
            filter: Some(GaiaFilter::default_capability_filter()),
        }
    }

    /// Create with custom filter.
    pub fn with_filter(filter: GaiaFilter) -> Self {
        Self {
            dataset_path: None,
            filter: Some(filter),
        }
    }

    fn default_dataset_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("datasets/gaia_sample.jsonl")
    }

    pub fn load_from_jsonl(path: &std::path::Path) -> anyhow::Result<Vec<Box<dyn EvalTask>>> {
        Self::load_from_jsonl_filtered(path, None)
    }

    pub fn load_from_jsonl_filtered(
        path: &std::path::Path,
        filter: Option<&GaiaFilter>,
    ) -> anyhow::Result<Vec<Box<dyn EvalTask>>> {
        let content = std::fs::read_to_string(path)?;
        let mut tasks: Vec<Box<dyn EvalTask>> = Vec::new();
        let mut total = 0u32;
        let mut excluded = 0u32;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let record: GaiaRecord = serde_json::from_str(line)?;
            total += 1;
            if let Some(f) = filter {
                if f.should_exclude(&record) {
                    excluded += 1;
                    continue;
                }
            }
            tasks.push(Box::new(GaiaTask::new(record)));
        }

        if excluded > 0 {
            eprintln!("GAIA: loaded {}/{total} tasks ({excluded} excluded by filter)", tasks.len());
        }

        Ok(tasks)
    }
}

impl ExternalBenchmark for GaiaBenchmark {
    fn name(&self) -> &str {
        "gaia"
    }

    fn description(&self) -> &str {
        "GAIA — General AI Assistants: multi-step reasoning + multi-tool evaluation (L1-L3)"
    }

    fn load_tasks(&self) -> anyhow::Result<Vec<Box<dyn EvalTask>>> {
        let path = self
            .dataset_path
            .clone()
            .unwrap_or_else(Self::default_dataset_path);

        if !path.exists() {
            anyhow::bail!(
                "GAIA dataset not found at {}. Download or create gaia_sample.jsonl.",
                path.display()
            );
        }

        Self::load_from_jsonl_filtered(&path, self.filter.as_ref())
    }

    fn custom_metrics(&self) -> Vec<MetricDefinition> {
        vec![
            MetricDefinition {
                name: "pass_rate_l1".into(),
                description: "Pass rate for Level 1 (easy) tasks".into(),
                unit: crate::benchmarks::MetricUnit::Percentage,
            },
            MetricDefinition {
                name: "pass_rate_l2".into(),
                description: "Pass rate for Level 2 (medium) tasks".into(),
                unit: crate::benchmarks::MetricUnit::Percentage,
            },
            MetricDefinition {
                name: "pass_rate_l3".into(),
                description: "Pass rate for Level 3 (hard) tasks".into(),
                unit: crate::benchmarks::MetricUnit::Percentage,
            },
        ]
    }
}

/// Pre-filtered GAIA benchmark — excludes image/audio/video tasks.
///
/// Registered as "gaia_filtered" in the benchmark registry.
/// Uses `GaiaFilter::default_capability_filter()` to exclude tasks
/// requiring OCR, audio transcription, or video understanding.
pub struct GaiaFilteredBenchmark {
    inner: GaiaBenchmark,
}

impl GaiaFilteredBenchmark {
    pub fn new() -> Self {
        Self {
            inner: GaiaBenchmark::with_default_filter(),
        }
    }
}

impl ExternalBenchmark for GaiaFilteredBenchmark {
    fn name(&self) -> &str {
        "gaia_filtered"
    }

    fn description(&self) -> &str {
        "GAIA (filtered) — excludes image/audio/video tasks that require unavailable capabilities"
    }

    fn load_tasks(&self) -> anyhow::Result<Vec<Box<dyn EvalTask>>> {
        self.inner.load_tasks()
    }

    fn custom_metrics(&self) -> Vec<MetricDefinition> {
        self.inner.custom_metrics()
    }
}

/// GAIA subset benchmark — loads from a pre-categorized JSONL file.
///
/// Used for category-specific evaluation (basic, file, web, reasoning, media).
pub struct GaiaSubsetBenchmark {
    suite_name: String,
    desc: String,
    dataset_file: String,
}

impl GaiaSubsetBenchmark {
    pub fn new(suite_name: &str, desc: &str, dataset_file: &str) -> Self {
        Self {
            suite_name: suite_name.to_string(),
            desc: desc.to_string(),
            dataset_file: dataset_file.to_string(),
        }
    }

    fn dataset_path(&self) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("datasets")
            .join(&self.dataset_file)
    }

    /// Create all standard GAIA subset benchmarks.
    pub fn all_subsets() -> Vec<Self> {
        vec![
            Self::new("gaia_core", "GAIA core — all evaluable tasks (excludes media blind spots)", "gaia_core.jsonl"),
            Self::new("gaia_basic", "GAIA basic — L1 single-step reasoning tasks", "gaia_basic.jsonl"),
            Self::new("gaia_file", "GAIA file — tasks requiring file parsing (xlsx/pdf/csv/docx)", "gaia_file.jsonl"),
            Self::new("gaia_web", "GAIA web — tasks requiring precise web data lookup", "gaia_web.jsonl"),
            Self::new("gaia_reasoning", "GAIA reasoning — L2/L3 multi-step reasoning without files", "gaia_reasoning.jsonl"),
            Self::new("gaia_media", "GAIA media — blind spot tasks (image/audio/video, reference only)", "gaia_media.jsonl"),
        ]
    }
}

impl ExternalBenchmark for GaiaSubsetBenchmark {
    fn name(&self) -> &str {
        &self.suite_name
    }

    fn description(&self) -> &str {
        &self.desc
    }

    fn load_tasks(&self) -> anyhow::Result<Vec<Box<dyn EvalTask>>> {
        let path = self.dataset_path();
        if !path.exists() {
            anyhow::bail!(
                "GAIA subset dataset not found at {}. Run dataset generation script.",
                path.display()
            );
        }
        GaiaBenchmark::load_from_jsonl(&path)
    }

    fn custom_metrics(&self) -> Vec<MetricDefinition> {
        vec![
            MetricDefinition {
                name: "pass_rate_l1".into(),
                description: "Pass rate for Level 1 (easy) tasks".into(),
                unit: crate::benchmarks::MetricUnit::Percentage,
            },
            MetricDefinition {
                name: "pass_rate_l2".into(),
                description: "Pass rate for Level 2 (medium) tasks".into(),
                unit: crate::benchmarks::MetricUnit::Percentage,
            },
            MetricDefinition {
                name: "pass_rate_l3".into(),
                description: "Pass rate for Level 3 (hard) tasks".into(),
                unit: crate::benchmarks::MetricUnit::Percentage,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gaia_record_deserialize() {
        let json = r#"{"task_id":"gaia-L1-001","question":"How many studios?","final_answer":"3","level":1}"#;
        let record: GaiaRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.task_id, "gaia-L1-001");
        assert_eq!(record.level, 1);
        assert_eq!(record.final_answer, "3");
    }

    #[test]
    fn test_gaia_difficulty_classification() {
        assert_eq!(GaiaTask::classify_difficulty(1), Difficulty::Easy);
        assert_eq!(GaiaTask::classify_difficulty(2), Difficulty::Medium);
        assert_eq!(GaiaTask::classify_difficulty(3), Difficulty::Hard);
    }

    #[test]
    fn test_gaia_scoring() {
        let record = GaiaRecord {
            task_id: "test-001".into(),
            question: "What is 2+2?".into(),
            final_answer: "4".into(),
            level: 1,
            annotator_metadata: None,
            file_name: None,
        };
        let task = GaiaTask::new(record);

        // Exact match pass case
        let output = AgentOutput {
            messages: vec![octo_types::ChatMessage::assistant("FINAL ANSWER: 4")],
            ..Default::default()
        };
        let score = task.score(&output);
        assert!(score.passed);
        assert_eq!(score.score, 1.0);

        // Exact match with normalization (trailing punctuation stripped)
        let output_norm = AgentOutput {
            messages: vec![octo_types::ChatMessage::assistant("FINAL ANSWER:  4.  ")],
            ..Default::default()
        };
        let score_norm = task.score(&output_norm);
        assert!(score_norm.passed);
        assert_eq!(score_norm.score, 1.0);

        // "The answer is X" pattern — should PASS via extract_answer
        let output_contains = AgentOutput {
            messages: vec![octo_types::ChatMessage::assistant("After analysis, the answer is 4.")],
            ..Default::default()
        };
        let score_contains = task.score(&output_contains);
        assert!(score_contains.passed, "extract_answer should find 'the answer is 4'");

        // FINAL ANSWER with extra text before it — should PASS
        let output_verbose = AgentOutput {
            messages: vec![octo_types::ChatMessage::assistant(
                "I checked multiple sources and confirmed.\n\nFINAL ANSWER: 4"
            )],
            ..Default::default()
        };
        let score_verbose = task.score(&output_verbose);
        assert!(score_verbose.passed);
        assert_eq!(score_verbose.score, 1.0);

        // Complete mismatch
        let output_fail = AgentOutput {
            messages: vec![octo_types::ChatMessage::assistant("I don't know.")],
            ..Default::default()
        };
        let score_fail = task.score(&output_fail);
        assert!(!score_fail.passed);
        assert_eq!(score_fail.score, 0.0);
    }

    #[test]
    fn test_normalize_answer() {
        assert_eq!(normalize_answer("  Hello.  "), "hello");
        assert_eq!(normalize_answer("Answer,"), "answer");
        assert_eq!(normalize_answer("YES;"), "yes");
        assert_eq!(normalize_answer("  42  "), "42");
        assert_eq!(normalize_answer("New York City"), "new york city");
        assert_eq!(normalize_answer("\"quoted\""), "quoted");
    }

    #[test]
    fn test_extract_answer() {
        // FINAL ANSWER: pattern
        assert_eq!(
            extract_answer("Some analysis here.\n\nFINAL ANSWER: 42"),
            "42"
        );
        // Case insensitive
        assert_eq!(
            extract_answer("Reasoning...\nfinal answer: New York"),
            "New York"
        );
        // With markdown bold
        assert_eq!(
            extract_answer("**FINAL ANSWER:** Paris"),
            "Paris"
        );
        // "The answer is" pattern
        assert_eq!(
            extract_answer("Based on my research, the answer is 7."),
            "7"
        );
        // Last line fallback
        assert_eq!(
            extract_answer("Some analysis\n\n42"),
            "42"
        );
        // Multi-line with FINAL ANSWER in middle
        assert_eq!(
            extract_answer("Step 1: search\nStep 2: verify\nFINAL ANSWER: Tokyo\nDone."),
            "Tokyo"
        );
    }

    #[test]
    fn test_fuzzy_match() {
        // Exact match
        assert_eq!(fuzzy_match("42", "42"), (true, 1.0));
        // Contains match (expected in actual)
        assert_eq!(fuzzy_match("paris", "the city is paris").0, true);
        // Numeric with commas
        assert_eq!(fuzzy_match("1000", "1000").0, true);
        // No match
        assert_eq!(fuzzy_match("paris", "london").0, false);
    }

    #[test]
    fn test_parse_number() {
        assert_eq!(parse_number("42"), Some(42.0));
        assert_eq!(parse_number("1,000"), Some(1000.0));
        assert_eq!(parse_number("$99.99"), Some(99.99));
        assert!(parse_number("hello").is_none());
    }

    #[test]
    fn test_gaia_benchmark_trait() {
        let bm = GaiaBenchmark::new();
        assert_eq!(bm.name(), "gaia");
        assert!(!bm.requires_sandbox());
        assert!(bm.sandbox_available());
        assert!(bm.custom_verifier().is_none());
        assert_eq!(bm.custom_metrics().len(), 3);
    }

    #[test]
    fn test_gaia_filter_excludes_media_files() {
        let filter = GaiaFilter::default_capability_filter();

        let png_record = GaiaRecord {
            task_id: "t1".into(), question: "What is in this image?".into(),
            final_answer: "cat".into(), level: 2,
            annotator_metadata: None, file_name: Some("image.png".into()),
        };
        assert!(filter.should_exclude(&png_record));

        let mp3_record = GaiaRecord {
            task_id: "t2".into(), question: "What is said?".into(),
            final_answer: "hello".into(), level: 2,
            annotator_metadata: None, file_name: Some("audio.mp3".into()),
        };
        assert!(filter.should_exclude(&mp3_record));

        let jpg_record = GaiaRecord {
            task_id: "t3".into(), question: "Count items".into(),
            final_answer: "5".into(), level: 1,
            annotator_metadata: None, file_name: Some("photo.JPG".into()),
        };
        assert!(filter.should_exclude(&jpg_record));
    }

    #[test]
    fn test_gaia_filter_excludes_youtube() {
        let filter = GaiaFilter::default_capability_filter();

        let yt_record = GaiaRecord {
            task_id: "t4".into(),
            question: "Watch https://youtube.com/watch?v=abc and tell me".into(),
            final_answer: "42".into(), level: 2,
            annotator_metadata: None, file_name: None,
        };
        assert!(filter.should_exclude(&yt_record));
    }

    #[test]
    fn test_gaia_filter_keeps_valid_tasks() {
        let filter = GaiaFilter::default_capability_filter();

        let text_record = GaiaRecord {
            task_id: "t5".into(), question: "What is 2+2?".into(),
            final_answer: "4".into(), level: 1,
            annotator_metadata: None, file_name: None,
        };
        assert!(!filter.should_exclude(&text_record));

        let xlsx_record = GaiaRecord {
            task_id: "t6".into(), question: "Sum column A".into(),
            final_answer: "100".into(), level: 2,
            annotator_metadata: None, file_name: Some("data.xlsx".into()),
        };
        assert!(!filter.should_exclude(&xlsx_record));

        let csv_record = GaiaRecord {
            task_id: "t7".into(), question: "Count rows".into(),
            final_answer: "10".into(), level: 1,
            annotator_metadata: None, file_name: Some("data.csv".into()),
        };
        assert!(!filter.should_exclude(&csv_record));
    }

    #[test]
    fn test_gaia_subsets_load_and_sum_to_total() {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("datasets");
        let subsets = [
            ("gaia_basic.jsonl", 31),
            ("gaia_file.jsonl", 22),
            ("gaia_web.jsonl", 44),
            ("gaia_reasoning.jsonl", 45),
            ("gaia_media.jsonl", 23),
        ];
        let mut total = 0;
        for (file, expected) in subsets {
            let path = base.join(file);
            if !path.exists() { return; }
            let tasks = GaiaBenchmark::load_from_jsonl(&path).unwrap();
            assert_eq!(tasks.len(), expected, "{file} count mismatch");
            total += tasks.len();
        }
        assert_eq!(total, 165, "subsets must sum to full dataset");

        // gaia_core = total - media
        let core_path = base.join("gaia_core.jsonl");
        if core_path.exists() {
            let core = GaiaBenchmark::load_from_jsonl(&core_path).unwrap();
            assert_eq!(core.len(), 142, "gaia_core count mismatch");
        }
    }

    #[test]
    fn test_gaia_filter_on_real_dataset() {
        let path = GaiaBenchmark::default_dataset_path();
        if !path.exists() {
            return; // skip if dataset not available
        }
        // Without filter
        let all_tasks = GaiaBenchmark::load_from_jsonl(&path).unwrap();
        assert_eq!(all_tasks.len(), 165);

        // With default capability filter
        let filter = GaiaFilter::default_capability_filter();
        let filtered = GaiaBenchmark::load_from_jsonl_filtered(&path, Some(&filter)).unwrap();
        // Should exclude ~16 tasks (10 image + 3 mp3 + 1 pptx + 9 youtube, minus overlaps)
        assert!(filtered.len() < all_tasks.len());
        assert!(filtered.len() >= 140, "filtered count {} too low", filtered.len());
        assert!(filtered.len() <= 155, "filtered count {} too high", filtered.len());
    }
}
