//! Validation checks.

mod parquet;

use crate::{Context, Finding};

/// A check that only needs the file footer.
pub type Check = fn(&Context) -> Vec<Finding>;

/// Returns the checks that run against the footer alone.
pub fn footer_checks() -> Vec<Check> {
    vec![parquet::info]
}

/// Runs every footer check and concatenates the findings.
pub fn run_footer_checks(context: &Context) -> Vec<Finding> {
    footer_checks()
        .into_iter()
        .flat_map(|check| check(context))
        .collect()
}
