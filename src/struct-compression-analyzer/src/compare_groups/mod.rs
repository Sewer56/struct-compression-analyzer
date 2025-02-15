pub(crate) mod test_helpers;
pub mod write_array;

use crate::split_comparisons::GroupComparisonMetrics;
use crate::split_comparisons::GroupDifference;

/// The result of comparing 2 arbitrary groups of fields based on the schema.
#[derive(Clone)]
pub struct GroupComparisonResult {
    /// The name of the group comparison. (Copied from schema)
    pub name: String,
    /// A description of the group comparison. (Copied from schema)
    pub description: String,
    /// The metrics for the first group.
    pub group1_metrics: GroupComparisonMetrics,
    /// The metrics for the second group.
    pub group2_metrics: GroupComparisonMetrics,
    /// Comparison between group 2 and group 1.
    pub difference: GroupDifference,
}
