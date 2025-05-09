use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Serialize)]
pub struct NodeMetrics {
    pub nodename: String,
    pub explorer_url: String,
    pub send_time: Duration,
    pub confirm_time: Duration,
}

#[derive(Debug, Serialize)]
pub struct BenchmarkResults {
    pub node_metrics: Vec<NodeMetrics>,
    pub total_transactions: usize,
}

impl BenchmarkResults {
    pub fn new() -> Self {
        Self {
            node_metrics: Vec::new(),
            total_transactions: 0,
        }
    }

    pub fn add_metrics(&mut self, metrics: NodeMetrics) {
        self.total_transactions += 1;
        self.node_metrics.push(metrics);
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self)
            .unwrap_or_else(|_| "Error serializing benchmark results".to_string())
    }
}
