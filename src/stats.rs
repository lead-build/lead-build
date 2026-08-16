use std::collections::HashMap;

use log::info;

use crate::path::VirtPath;

#[derive(Debug, Default)]
pub struct NinjaFileStats {
    rule_additions: HashMap<String, usize>,
    output_additions: HashMap<VirtPath, usize>,
}

impl NinjaFileStats {
    pub fn record_rule_addition(&mut self, rule_name: &str) {
        *self
            .rule_additions
            .entry(rule_name.to_string())
            .or_insert(0) += 1;
    }

    pub fn record_output_addition(&mut self, output: &VirtPath) {
        *self.output_additions.entry(output.clone()).or_insert(0) += 1;
    }

    pub fn log_summary(&self) {
        let mut rules = self
            .rule_additions
            .iter()
            .map(|(name, count)| (name.clone(), *count))
            .collect::<Vec<_>>();
        rules.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));

        let mut outputs = self
            .output_additions
            .iter()
            .map(|(path, count)| (path.to_path_buf().display().to_string(), *count))
            .collect::<Vec<_>>();
        outputs.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));

        info!("stats rules added:");
        if rules.is_empty() {
            info!("  <none>");
        } else {
            info!("  {:>8} | rule", "count");
            info!("  {}", "-".repeat(8 + 3 + 4));
            for (name, count) in rules.iter() {
                info!("  {:>8} | {}", count, name);
            }
        }

        info!("stats outputs added:");
        if outputs.is_empty() {
            info!("  <none>");
        } else {
            info!("  {:>8} | output", "count");
            info!("  {}", "-".repeat(8 + 3 + 6));
            for (path, count) in outputs.iter() {
                info!("  {:>8} | {}", count, path);
            }
        }
    }
}
