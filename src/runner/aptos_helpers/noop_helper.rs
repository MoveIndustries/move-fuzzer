use crate::mutator::types::Type as FuzzerType;
use crate::runner::aptos_helpers::AptosHelper;

pub struct NoopHelper;

impl NoopHelper {
    pub fn new() -> Self {
        Self
    }
}

impl AptosHelper for NoopHelper {
    fn transform_inputs(&self, inputs: &[FuzzerType]) -> Option<Vec<FuzzerType>> {
        Some(inputs.to_vec())
    }
}
