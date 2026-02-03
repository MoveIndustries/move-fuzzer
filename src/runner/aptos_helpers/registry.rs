use std::collections::{HashMap, HashSet};

use super::AptosHelper;
use super::noop_helper::NoopHelper;

pub fn build_helpers(
    aptos_helpers: &HashMap<String, String>,
    seed: u64,
) -> HashMap<String, Box<dyn AptosHelper>> {
    let mut unique = HashSet::new();
    for name in aptos_helpers.values() {
        unique.insert(name.as_str());
    }

    let mut helpers: HashMap<String, Box<dyn AptosHelper>> = HashMap::new();
    for name in unique {
        match name {
            "noop_v1" => {
                let _ = seed;
                helpers.insert(name.to_string(), Box::new(NoopHelper::new()));
            }
            _ => {}
        }
    }
    helpers
}
