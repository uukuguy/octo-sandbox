//! External benchmark adaptation layer.
//!
//! Provides a pluggable trait system for integrating industry-standard benchmarks
//! (GAIA, SWE-bench, τ-bench, etc.) into the octo-eval pipeline.

pub mod gaia;
pub mod swe_bench;
pub mod tau_bench;
pub mod terminal_bench;

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use crate::score::EvalScore;
use crate::task::{AgentOutput, EvalTask};

/// External benchmark adapter trait — shared interface for all external benchmarks.
pub trait ExternalBenchmark: Send + Sync {
    /// Benchmark name (e.g., "gaia", "swe_bench", "tau_bench")
    fn name(&self) -> &str;

    /// Human-readable description
    fn description(&self) -> &str;

    /// Load tasks from the benchmark dataset
    fn load_tasks(&self) -> anyhow::Result<Vec<Box<dyn EvalTask>>>;

    /// Whether this benchmark requires a special sandbox environment (Docker, VM, etc.)
    fn requires_sandbox(&self) -> bool {
        false
    }

    /// Whether the required sandbox is available at runtime
    fn sandbox_available(&self) -> bool {
        true
    }

    /// Custom verifier (overrides default `task.score()` when present)
    fn custom_verifier(&self) -> Option<Box<dyn BenchmarkVerifier>> {
        None
    }

    /// Custom evaluation metrics (e.g., τ-bench pass^k)
    fn custom_metrics(&self) -> Vec<MetricDefinition> {
        vec![]
    }
}

/// Verifier trait — for benchmarks needing external verification (e.g., SWE-bench Docker)
pub trait BenchmarkVerifier: Send + Sync {
    fn verify<'a>(
        &'a self,
        task: &'a dyn EvalTask,
        output: &'a AgentOutput,
    ) -> Pin<Box<dyn Future<Output = EvalScore> + Send + 'a>>;
}

/// Custom metric definition for benchmark-specific measurements
#[derive(Debug, Clone)]
pub struct MetricDefinition {
    pub name: String,
    pub description: String,
    pub unit: MetricUnit,
}

#[derive(Debug, Clone)]
pub enum MetricUnit {
    Percentage,
    Count,
    Seconds,
    Custom(String),
}

/// Registry of external benchmarks
pub struct BenchmarkRegistry {
    benchmarks: HashMap<String, Box<dyn ExternalBenchmark>>,
}

impl BenchmarkRegistry {
    pub fn new() -> Self {
        Self {
            benchmarks: HashMap::new(),
        }
    }

    /// Create a registry pre-populated with all built-in benchmark adapters.
    pub fn with_defaults() -> Self {
        let mut reg = Self::new();
        reg.register(Box::new(gaia::GaiaBenchmark::new()));
        reg.register(Box::new(swe_bench::SweBenchmark::new()));
        reg.register(Box::new(tau_bench::TauBenchmark::new()));
        reg.register(Box::new(terminal_bench::TerminalBenchmark::new()));
        reg
    }

    pub fn register(&mut self, benchmark: Box<dyn ExternalBenchmark>) {
        self.benchmarks
            .insert(benchmark.name().to_string(), benchmark);
    }

    pub fn get(&self, name: &str) -> Option<&dyn ExternalBenchmark> {
        self.benchmarks.get(name).map(|b| b.as_ref())
    }

    pub fn list(&self) -> Vec<&dyn ExternalBenchmark> {
        self.benchmarks.values().map(|b| b.as_ref()).collect()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.benchmarks.contains_key(name)
    }
}

impl Default for BenchmarkRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyBenchmark;

    impl ExternalBenchmark for DummyBenchmark {
        fn name(&self) -> &str {
            "dummy"
        }
        fn description(&self) -> &str {
            "A dummy benchmark for testing"
        }
        fn load_tasks(&self) -> anyhow::Result<Vec<Box<dyn EvalTask>>> {
            Ok(vec![])
        }
    }

    #[test]
    fn test_registry_register_and_get() {
        let mut reg = BenchmarkRegistry::new();
        reg.register(Box::new(DummyBenchmark));

        assert!(reg.contains("dummy"));
        assert!(!reg.contains("nonexistent"));

        let bm = reg.get("dummy").unwrap();
        assert_eq!(bm.name(), "dummy");
        assert_eq!(bm.description(), "A dummy benchmark for testing");
    }

    #[test]
    fn test_registry_list() {
        let mut reg = BenchmarkRegistry::new();
        reg.register(Box::new(DummyBenchmark));
        let listed = reg.list();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name(), "dummy");
    }

    #[test]
    fn test_default_methods() {
        let bm = DummyBenchmark;
        assert!(!bm.requires_sandbox());
        assert!(bm.sandbox_available());
        assert!(bm.custom_verifier().is_none());
        assert!(bm.custom_metrics().is_empty());
    }
}
