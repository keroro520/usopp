use serde::Serialize;
use solana_sdk::signature::Signature;
use std::time::Duration;

#[derive(Debug, Serialize)]
pub struct NodeMetrics {
    pub node_url: String,
    pub signature: Signature,
    pub build_time: Duration,
    pub send_time: Duration,
    pub confirm_time: Duration,
    pub status: TransactionStatus,
}

#[derive(Debug, Serialize)]
pub enum TransactionStatus {
    Success,
    Failed(String),
}

#[derive(Debug, Serialize)]
pub struct BenchmarkResults {
    pub node_metrics: Vec<NodeMetrics>,
    pub total_transactions: usize,
    pub successful_transactions: usize,
    pub failed_transactions: usize,
    pub average_build_time: Duration,
    pub average_send_time: Duration,
    pub average_confirm_time: Duration,
}

impl BenchmarkResults {
    pub fn new() -> Self {
        Self {
            node_metrics: Vec::new(),
            total_transactions: 0,
            successful_transactions: 0,
            failed_transactions: 0,
            average_build_time: Duration::from_millis(0),
            average_send_time: Duration::from_millis(0),
            average_confirm_time: Duration::from_millis(0),
        }
    }

    pub fn add_metrics(&mut self, metrics: NodeMetrics) {
        self.total_transactions += 1;
        match metrics.status {
            TransactionStatus::Success => self.successful_transactions += 1,
            TransactionStatus::Failed(_) => self.failed_transactions += 1,
        }

        // Update averages
        let n = self.node_metrics.len() as u32;
        self.average_build_time = (self.average_build_time * n + metrics.build_time) / (n + 1);
        self.average_send_time = (self.average_send_time * n + metrics.send_time) / (n + 1);
        self.average_confirm_time =
            (self.average_confirm_time * n + metrics.confirm_time) / (n + 1);

        self.node_metrics.push(metrics);
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self)
            .unwrap_or_else(|_| "Error serializing results".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::signature::Signature;

    #[test]
    fn test_benchmark_results() {
        let mut results = BenchmarkResults::new();

        let metrics = NodeMetrics {
            node_url: "https://test.com".to_string(),
            signature: Signature::default(),
            build_time: Duration::from_millis(100),
            send_time: Duration::from_millis(200),
            confirm_time: Duration::from_millis(300),
            status: TransactionStatus::Success,
        };

        results.add_metrics(metrics);

        assert_eq!(results.total_transactions, 1);
        assert_eq!(results.successful_transactions, 1);
        assert_eq!(results.failed_transactions, 0);
        assert_eq!(results.average_build_time, Duration::from_millis(100));
        assert_eq!(results.average_send_time, Duration::from_millis(200));
        assert_eq!(results.average_confirm_time, Duration::from_millis(300));
    }
}
