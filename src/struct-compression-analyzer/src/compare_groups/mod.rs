#[cfg(test)]
pub(crate) mod test_helpers;
pub mod write_array;
pub mod write_struct;

use crate::split_comparisons::GroupComparisonMetrics;
use crate::split_comparisons::GroupDifference;

/// The result of comparing 2 arbitrary groups of fields based on the schema.
#[derive(Clone)]
pub struct GroupComparisonResult {
    /// The name of the group comparison. (Copied from schema)
    pub name: String,
    /// A description of the group comparison. (Copied from schema)
    pub description: String,
    /// Metrics for the baseline group.
    pub baseline_metrics: GroupComparisonMetrics,
    /// Metrics for the other groups.
    pub group_metrics: Vec<GroupComparisonMetrics>,
    /// Comparison between other groups and first (baseline) group.
    pub differences: Vec<GroupDifference>,
}
