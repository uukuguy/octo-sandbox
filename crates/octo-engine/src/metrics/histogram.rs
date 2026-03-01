use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

/// Histogram metric - distribution of values
#[derive(Clone)]
pub struct Histogram(Arc<HistogramInner>);

struct HistogramInner {
    buckets: Vec<f64>,
    counts: Vec<AtomicU64>,
    sum: AtomicU64,
    count: AtomicUsize,
}

impl Histogram {
    pub fn new(buckets: Vec<f64>) -> Self {
        let mut sorted_buckets = buckets;
        sorted_buckets.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let bucket_count = sorted_buckets.len() + 1;
        let counts: Vec<AtomicU64> = (0..bucket_count).map(|_| AtomicU64::new(0)).collect();

        Self(Arc::new(HistogramInner {
            buckets: sorted_buckets,
            counts,
            sum: AtomicU64::new(0),
            count: AtomicUsize::new(0),
        }))
    }

    pub fn observe(&self, value: f64) {
        // Find the bucket index
        let bucket_idx = self.0.buckets.iter().position(|&b| value <= b).unwrap_or(self.0.buckets.len());
        self.0.counts[bucket_idx].fetch_add(1, Ordering::Relaxed);

        // Add to sum (truncating to u64 for simplicity)
        self.0.sum.fetch_add(value as u64, Ordering::Relaxed);
        self.0.count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn count(&self) -> usize {
        self.0.count.load(Ordering::Relaxed)
    }

    pub fn sum(&self) -> f64 {
        self.0.sum.load(Ordering::Relaxed) as f64
    }

    pub fn mean(&self) -> f64 {
        let count = self.count();
        if count == 0 {
            return 0.0;
        }
        self.sum() / count as f64
    }

    /// Get bucket values for Prometheus-style histogram output
    pub fn buckets(&self) -> Vec<(f64, u64)> {
        self.0.buckets
            .iter()
            .zip(self.0.counts.iter())
            .map(|(&bucket, count)| (bucket, count.load(Ordering::Relaxed)))
            .collect()
    }

    /// Get cumulative bucket values (each bucket includes all previous buckets)
    pub fn cumulative_buckets(&self) -> Vec<(f64, u64)> {
        let mut cumulative = 0u64;
        self.0.buckets
            .iter()
            .zip(self.0.counts.iter())
            .map(|(&bucket, count)| {
                cumulative += count.load(Ordering::Relaxed);
                (bucket, cumulative)
            })
            .collect()
    }

    pub fn reset(&self) {
        for count in &self.0.counts {
            count.store(0, Ordering::Relaxed);
        }
        self.0.sum.store(0, Ordering::Relaxed);
        self.0.count.store(0, Ordering::Relaxed);
    }
}

impl std::fmt::Debug for Histogram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Histogram")
            .field("count", &self.count())
            .field("sum", &self.sum())
            .field("mean", &self.mean())
            .finish()
    }
}
