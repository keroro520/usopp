use crate::websocket::ConfirmationResult;
use std::collections::{BTreeMap, BTreeSet};

pub type NodeName = String;
pub type NodeConfirmationResults = Vec<ConfirmationResult>;

fn format_duration_us(us: u64) -> String {
    match us {
        0 => "0 μs".to_string(),
        us if us < 1_000 => format!("{} μs", us),
        us if us < 1_000_000 => format!("{:.2} ms", us as f64 / 1_000.0),
        _ => format!("{:.2} s", us as f64 / 1_000_000.0),
    }
}

/// Generate a markdown report comparing confirmation times per signature across nodes.
pub fn generate_report_markdown(
    all_node_confirmations: &[(NodeName, NodeConfirmationResults)],
) -> String {
    // Step 1: Aggregate data by signature
    // signature -> node_name -> timestamp_us
    let mut signature_map: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let mut all_node_names: BTreeSet<String> = BTreeSet::new();

    for (node_name, confirmations) in all_node_confirmations {
        all_node_names.insert(node_name.clone());
        for conf in confirmations {
            signature_map
                .entry(conf.signature.clone())
                .or_default()
                .insert(node_name.clone(), conf.timestamp_us);
        }
    }

    // Step 2: Build per-signature delta table
    let mut md = String::new();
    md.push_str("## Per-Signature Δ from Fastest\n\n");
    md.push_str("| Signature ");
    for node in &all_node_names {
        md.push_str(&format!("| {} (Δ) ", node));
    }
    md.push_str("|\n");
    md.push_str("|---");
    for _ in &all_node_names {
        md.push_str("|---");
    }
    md.push_str("|\n");

    // For sum(Δ) calculation
    let mut node_sum_delta: BTreeMap<String, u64> = BTreeMap::new();

    for (signature, node_map) in &signature_map {
        md.push_str(&format!("| {} ", signature));
        // Find the fastest timestamp for this signature
        let min_ts = node_map.values().min().copied();
        for node in &all_node_names {
            match (node_map.get(node), min_ts) {
                (Some(&ts), Some(min)) => {
                    let delta = ts.saturating_sub(min);
                    *node_sum_delta.entry(node.clone()).or_insert(0) += delta;
                    md.push_str(&format!("| {} ", format_duration_us(delta)));
                }
                _ => {
                    md.push_str("| N/A ");
                }
            }
        }
        md.push_str("|\n");
    }

    // Step 3: Build node sum(Δ) table
    md.push_str("\n## Node Performance Summary (Lower Sum Δ is Better)\n\n");
    md.push_str(
        "| Order | Node Name | Sum Δ |
|---|---|---|
",
    );
    let mut node_sum_vec: Vec<_> = node_sum_delta.into_iter().collect();
    node_sum_vec.sort_by_key(|&(_, sum)| sum);
    for (i, (node, sum)) in node_sum_vec.iter().enumerate() {
        md.push_str(&format!(
            "| {} | {} | {} |\n",
            i + 1,
            node,
            format_duration_us(*sum)
        ));
    }

    md
}
