use std::sync::Arc;

use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
pub struct MetricsSnapshot {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub counters: Vec<CounterMetric>,
    pub gauges: Vec<GaugeMetric>,
    pub histograms: Vec<HistogramMetric>,
}

#[derive(Serialize)]
pub struct CounterMetric {
    pub name: String,
    pub value: u64,
}

#[derive(Serialize)]
pub struct GaugeMetric {
    pub name: String,
    pub value: i64,
}

#[derive(Serialize)]
pub struct HistogramMetric {
    pub name: String,
    pub count: u64,
    pub sum: f64,
    pub buckets: Vec<Bucket>,
}

#[derive(Serialize)]
pub struct Bucket {
    pub le: f64,
    pub count: u64,
}

pub async fn get_metrics(State(state): State<Arc<AppState>>) -> Json<MetricsSnapshot> {
    let registry = state.metrics_registry.read().await;

    let counters = registry
        .counters()
        .iter()
        .map(|e| CounterMetric {
            name: e.key().clone(),
            value: e.value().get(),
        })
        .collect();

    let gauges = registry
        .gauges()
        .iter()
        .map(|e| GaugeMetric {
            name: e.key().clone(),
            value: e.value().get(),
        })
        .collect();

    let histograms = registry
        .histograms()
        .iter()
        .map(|e| {
            let snapshot = e.value().snapshot();
            HistogramMetric {
                name: e.key().clone(),
                count: snapshot.count,
                sum: snapshot.sum,
                buckets: snapshot
                    .buckets
                    .iter()
                    .map(|b| Bucket {
                        le: b.le,
                        count: b.cumulative_count,
                    })
                    .collect(),
            }
        })
        .collect();

    Json(MetricsSnapshot {
        timestamp: chrono::Utc::now(),
        counters,
        gauges,
        histograms,
    })
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/metrics", get(get_metrics))
}
