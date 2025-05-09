use std::collections::{BTreeSet, HashMap};

// Assuming ConfirmationResult is defined in websocket.rs and accessible via crate::websocket::ConfirmationResult
// If the path is different, this use statement will need to be adjusted.
// If ConfirmationResult is not public or not in this path, its definition might need to be
// made available or duplicated (though less ideal).
use crate::websocket::ConfirmationResult;

// Type aliases for clarity, matching what might be used elsewhere or for local convenience.
type NodeName = String;
type NodeConfirmationResults = Vec<ConfirmationResult>;

struct ReportData {
    sorted_node_names: Vec<NodeName>,
    sorted_signatures: Vec<String>,
    signature_node_scores: HashMap<String, HashMap<NodeName, u32>>,
    node_total_scores: HashMap<NodeName, u32>,
}

fn prepare_and_calculate_scores(
    all_node_confirmations: &[(NodeName, NodeConfirmationResults)],
) -> ReportData {
    let mut node_names_set: BTreeSet<NodeName> = BTreeSet::new();
    let mut all_signatures_set: BTreeSet<String> = BTreeSet::new();
    let mut raw_confirmations_by_sig: HashMap<String, Vec<(NodeName, u64)>> = HashMap::new();

    for (node_name, results) in all_node_confirmations {
        node_names_set.insert(node_name.clone());
        for conf_result in results {
            all_signatures_set.insert(conf_result.signature.clone());
            raw_confirmations_by_sig
                .entry(conf_result.signature.clone())
                .or_default()
                .push((node_name.clone(), conf_result.timestamp_us));
        }
    }

    let sorted_node_names: Vec<NodeName> = node_names_set.into_iter().collect();
    let sorted_signatures: Vec<String> = all_signatures_set.into_iter().collect();

    let mut signature_node_scores: HashMap<String, HashMap<NodeName, u32>> = HashMap::new();
    let mut node_total_scores: HashMap<NodeName, u32> = HashMap::new();
    for node_name in &sorted_node_names {
        node_total_scores.insert(node_name.clone(), 0);
    }

    for sig in &sorted_signatures {
        if let Some(confirmations_for_sig_ref) = raw_confirmations_by_sig.get(sig) {
            let mut confirmations_for_sig = confirmations_for_sig_ref.clone();
            confirmations_for_sig.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));

            let mut scores_for_this_sig: HashMap<NodeName, u32> = HashMap::new();
            for (rank, (node_name, _timestamp)) in confirmations_for_sig.iter().enumerate() {
                let score = (rank + 1) as u32;
                scores_for_this_sig.insert(node_name.clone(), score);
                *node_total_scores.entry(node_name.clone()).or_default() += score;
            }
            signature_node_scores.insert(sig.clone(), scores_for_this_sig);
        }
    }

    ReportData {
        sorted_node_names,
        sorted_signatures,
        signature_node_scores,
        node_total_scores,
    }
}

fn build_signature_table_markdown(
    sorted_signatures: &[String],
    sorted_node_names: &[NodeName],
    signature_node_scores: &HashMap<String, HashMap<NodeName, u32>>,
) -> String {
    let mut markdown_output = String::new();
    markdown_output.push_str("## Signature Confirmation Report\n\n");

    markdown_output.push_str("| Signature ");
    for node_name in sorted_node_names {
        markdown_output.push_str(&format!("| {} Score ", node_name));
    }
    markdown_output.push_str("|\n");

    markdown_output.push_str("|---");
    for _ in sorted_node_names {
        markdown_output.push_str("|---");
    }
    markdown_output.push_str("|\n");

    if sorted_signatures.is_empty() {
        markdown_output.push_str("| *No signatures confirmed* ");
        for _ in sorted_node_names {
            markdown_output.push_str("| - ");
        }
        markdown_output.push_str("|\n");
    } else {
        for sig in sorted_signatures {
            markdown_output.push_str(&format!("| {} ", sig));
            let scores_for_sig = signature_node_scores.get(sig);
            for node_name in sorted_node_names {
                let score_str = scores_for_sig
                    .and_then(|map| map.get(node_name))
                    .map_or("-".to_string(), |s| s.to_string());
                markdown_output.push_str(&format!("| {} ", score_str));
            }
            markdown_output.push_str("|\n");
        }
    }
    markdown_output
}

fn build_node_summary_table_markdown(node_total_scores: HashMap<NodeName, u32>) -> String {
    let mut markdown_output = String::new();
    markdown_output.push_str("\n## Node Performance Summary (Lower Sum Score is Better)\n\n");

    let mut sorted_node_performance: Vec<(NodeName, u32)> = node_total_scores.into_iter().collect();
    sorted_node_performance.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));

    markdown_output.push_str("| Order | Node Name | Sum Score |\n");
    markdown_output.push_str("|---|---|---|\n");

    if sorted_node_performance.is_empty() {
        markdown_output.push_str("| - | *No nodes to report* | - |\n");
    } else {
        for (rank, (node_name, sum_score)) in sorted_node_performance.iter().enumerate() {
            markdown_output.push_str(&format!(
                "| {} | {} | {} |\n",
                rank + 1,
                node_name,
                sum_score
            ));
        }
    }
    markdown_output
}

pub fn generate_report_markdown(
    all_node_confirmations: &[(NodeName, NodeConfirmationResults)],
) -> String {
    if all_node_confirmations.is_empty() && all_node_confirmations.iter().all(|(_, v)| v.is_empty())
    {
        // A more robust check for truly no data vs nodes existing but no confirmations for any.
        // The helper functions handle empty sorted_signatures/sorted_node_names gracefully already.
    }

    let report_data = prepare_and_calculate_scores(all_node_confirmations);

    let mut markdown_output = String::new();
    markdown_output.push_str(&build_signature_table_markdown(
        &report_data.sorted_signatures,
        &report_data.sorted_node_names,
        &report_data.signature_node_scores,
    ));
    markdown_output.push_str(&build_node_summary_table_markdown(
        report_data.node_total_scores,
    ));

    markdown_output
}

// ... (keeping existing tests, they should still work if ConfirmationResult is compatible)
#[cfg(test)]
mod tests {
    use super::*;
    // Assuming ConfirmationResult is accessible for tests.
    // If not, tests might need to be in the same crate or use a mock.
    // For now, this is a placeholder for where ConfirmationResult would be defined for tests:
    // #[derive(Debug, Clone)]
    // pub struct ConfirmationResult {
    //     pub signature: String,
    //     pub timestamp: u64,
    // }

    #[test]
    fn test_generate_report_markdown_empty_input() {
        let all_node_confirmations: Vec<(NodeName, NodeConfirmationResults)> = Vec::new();
        let report = generate_report_markdown(&all_node_confirmations);

        assert!(report.contains("## Signature Confirmation Report"));
        assert!(report.contains("| *No signatures confirmed* |")); // This check should be specific to the table content
        assert!(report.contains("## Node Performance Summary (Lower Sum Score is Better)"));
        assert!(report.contains("| - | *No nodes to report* | - |"));
    }

    #[test]
    fn test_generate_report_markdown_no_confirmations() {
        let all_node_confirmations: Vec<(NodeName, NodeConfirmationResults)> =
            vec![("node1".to_string(), vec![]), ("node2".to_string(), vec![])];
        let report = generate_report_markdown(&all_node_confirmations);

        assert!(report.contains("## Signature Confirmation Report"));
        assert!(report.contains("| Signature | node1 Score | node2 Score |"));
        assert!(report.contains("| *No signatures confirmed* | - | - |"));

        assert!(report.contains("## Node Performance Summary (Lower Sum Score is Better)"));
        assert!(report.contains("| Order | Node Name | Sum Score |"));
        assert!(report.contains("| 1 | node1 | 0 |")); // node1 comes before node2 alphabetically
        assert!(report.contains("| 2 | node2 | 0 |"));
    }

    #[test]
    fn test_generate_report_markdown_basic_data() {
        let all_node_confirmations: Vec<(NodeName, NodeConfirmationResults)> = vec![
            (
                "node1".to_string(),
                vec![
                    ConfirmationResult {
                        signature: "sigA".to_string(),
                        timestamp: 100,
                    },
                    ConfirmationResult {
                        signature: "sigB".to_string(),
                        timestamp: 102,
                    },
                ],
            ),
            (
                "node2".to_string(),
                vec![
                    ConfirmationResult {
                        signature: "sigA".to_string(),
                        timestamp: 101,
                    },
                    ConfirmationResult {
                        signature: "sigB".to_string(),
                        timestamp: 101,
                    },
                ],
            ),
            (
                "node3".to_string(), // Node3 confirms nothing for sigA, sigB
                vec![ConfirmationResult {
                    signature: "sigC".to_string(),
                    timestamp: 100,
                }],
            ),
        ];

        let report = generate_report_markdown(&all_node_confirmations);
        // println!("{}", report); // For debugging test output

        // Check Signature Report
        assert!(report.contains("## Signature Confirmation Report"));
        assert!(report.contains("| Signature | node1 Score | node2 Score | node3 Score |"));
        assert!(report.contains("| sigA | 1 | 2 | - |")); // node1 faster for sigA
        assert!(report.contains("| sigB | 2 | 1 | - |")); // node2 faster for sigB
        assert!(report.contains("| sigC | - | - | 1 |")); // only node3 for sigC

        // Check Node Performance Summary
        // Scores:
        // node1: sigA (1) + sigB (2) = 3
        // node2: sigA (2) + sigB (1) = 3
        // node3: sigC (1) = 1
        // Expected order: node3 (1), then node1 (3), then node2 (3) due to alphabetical tie-break
        assert!(report.contains("## Node Performance Summary (Lower Sum Score is Better)"));
        assert!(report.contains("| Order | Node Name | Sum Score |"));
        assert!(report.contains("| 1 | node3 | 1 |"));
        assert!(report.contains("| 2 | node1 | 3 |"));
        assert!(report.contains("| 3 | node2 | 3 |"));
    }

    #[test]
    fn test_timestamp_tie_breaking() {
        let all_node_confirmations: Vec<(NodeName, NodeConfirmationResults)> = vec![
            (
                "node_alpha".to_string(), // Confirms sigX at 100
                vec![ConfirmationResult {
                    signature: "sigX".to_string(),
                    timestamp: 100,
                }],
            ),
            (
                "node_beta".to_string(), // Confirms sigX at 100 (tie)
                vec![ConfirmationResult {
                    signature: "sigX".to_string(),
                    timestamp: 100,
                }],
            ),
        ];

        let report = generate_report_markdown(&all_node_confirmations);
        // println!("{}", report);
        // sigX: node_alpha (score 1, due to name), node_beta (score 2)
        // Note: The table header order depends on BTreeSet iteration of node names.
        // Assuming "node_alpha" comes before "node_beta"
        assert!(report.contains("| sigX | 1 | 2 |"));

        // Node summary:
        // node_alpha: 1
        // node_beta: 2
        assert!(report.contains("| 1 | node_alpha | 1 |"));
        assert!(report.contains("| 2 | node_beta | 2 |"));
    }
}
