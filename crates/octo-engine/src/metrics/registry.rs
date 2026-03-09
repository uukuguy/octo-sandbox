use dashmap::DashMap;

use super::{Counter, Gauge, Histogram};

/// Global metrics registry - manages all metric types
pub struct MetricsRegistry {
    counters: DashMap<String, Counter>,
    gauges: DashMap<String, Gauge>,
    histograms: DashMap<String, Histogram>,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self {
            counters: DashMap::new(),
            gauges: DashMap::new(),
            histograms: DashMap::new(),
        }
    }

    /// Get or create a counter metric
    pub fn counter(&self, name: &str) -> Counter {
        self.counters.entry(name.to_string()).or_default().clone()
    }

    /// Get or create a gauge metric
    pub fn gauge(&self, name: &str) -> Gauge {
        self.gauges.entry(name.to_string()).or_default().clone()
    }

    /// Get or create a histogram metric
    pub fn histogram(&self, name: &str, buckets: Vec<f64>) -> Histogram {
        self.histograms
            .entry(name.to_string())
            .or_insert_with(|| Histogram::new(buckets))
            .clone()
    }

    /// Get all counter names
    pub fn counter_names(&self) -> Vec<String> {
        self.counters.iter().map(|r| r.key().clone()).collect()
    }

    /// Get all gauge names
    pub fn gauge_names(&self) -> Vec<String> {
        self.gauges.iter().map(|r| r.key().clone()).collect()
    }

    /// Get all histogram names
    pub fn histogram_names(&self) -> Vec<String> {
        self.histograms.iter().map(|r| r.key().clone()).collect()
    }

    /// Get all counters as a DashMap reference for iteration
    pub fn counters(&self) -> &DashMap<String, Counter> {
        &self.counters
    }

    /// Get all gauges as a DashMap reference for iteration
    pub fn gauges(&self) -> &DashMap<String, Gauge> {
        &self.gauges
    }

    /// Get all histograms as a DashMap reference for iteration
    pub fn histograms(&self) -> &DashMap<String, Histogram> {
        &self.histograms
    }

    /// Clear all metrics
    pub fn clear(&self) {
        self.counters.clear();
        self.gauges.clear();
        self.histograms.clear();
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for MetricsRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MetricsRegistry")
            .field("counters", &self.counter_names())
            .field("gauges", &self.gauge_names())
            .field("histograms", &self.histogram_names())
            .finish()
    }
}
