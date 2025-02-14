use crate::{
    analysis_results::FieldMetrics,
    analyze_utils::{
        calculate_file_entropy, get_writer_buffer, get_zstd_compressed_size, size_estimate,
    },
    analyzer::FieldStats,
    schema::SplitComparison,
};
use ahash::AHashMap;
use lossless_transform_utils::match_estimator::estimate_num_lz_matches_fast;

pub fn calc_split_comparisons(
    field_stats: &mut AHashMap<String, FieldStats>,
    comparisons: &[SplitComparison],
    field_metrics: &AHashMap<String, FieldMetrics>,
) -> Vec<SplitComparisonResult> {
    let mut split_comparisons = Vec::new();
    for comparison in comparisons {
        let mut group1_bytes: Vec<u8> = Vec::new();
        let mut group2_bytes: Vec<u8> = Vec::new();

        // Sum up bytes for group 1
        for name in &comparison.group_1 {
            if let Some(stats) = field_stats.get_mut(name) {
                group1_bytes.extend_from_slice(get_writer_buffer(&mut stats.writer));
            }
        }

        // Sum up bytes for group 2
        for name in &comparison.group_2 {
            if let Some(stats) = field_stats.get_mut(name) {
                group2_bytes.extend_from_slice(get_writer_buffer(&mut stats.writer));
            }
        }

        let mut group1_field_metrics: Vec<FieldMetrics> = Vec::new();
        let mut group2_field_metrics: Vec<FieldMetrics> = Vec::new();
        for path in &comparison.group_1 {
            if let Some(metrics) = field_metrics.iter().find(|(_k, v)| v.name == *path) {
                group1_field_metrics.push(metrics.1.clone());
            }
        }
        for path in &comparison.group_2 {
            if let Some(metrics) = field_metrics.iter().find(|(_k, v)| v.name == *path) {
                group2_field_metrics.push(metrics.1.clone());
            }
        }

        // Calculate entropy and LZ matches for both group sets.
        let entropy1 = calculate_file_entropy(&group1_bytes);
        let entropy2 = calculate_file_entropy(&group2_bytes);
        let lz_matches1 = estimate_num_lz_matches_fast(&group1_bytes);
        let lz_matches2 = estimate_num_lz_matches_fast(&group2_bytes);
        let estimated_size_1 = size_estimate(&group1_bytes, lz_matches1, entropy1);
        let estimated_size_2 = size_estimate(&group2_bytes, lz_matches2, entropy2);
        let actual_size_1 = get_zstd_compressed_size(&group1_bytes);
        let actual_size_2 = get_zstd_compressed_size(&group2_bytes);

        split_comparisons.push(SplitComparisonResult {
            name: comparison.name.clone(),
            description: comparison.description.clone(),
            group1_metrics: GroupComparisonMetrics {
                lz_matches: lz_matches1 as u64,
                entropy: entropy1,
                estimated_size: estimated_size_1 as u64,
                zstd_size: actual_size_1 as u64,
                original_size: group1_bytes.len() as u64,
            },
            group2_metrics: GroupComparisonMetrics {
                lz_matches: lz_matches2 as u64,
                entropy: entropy2,
                estimated_size: estimated_size_2 as u64,
                zstd_size: actual_size_2 as u64,
                original_size: group2_bytes.len() as u64,
            },
            difference: GroupDifference {
                lz_matches: (lz_matches2.wrapping_sub(lz_matches1)) as i64,
                entropy: (entropy2 - entropy1).abs(),
                estimated_size: (estimated_size_2.wrapping_sub(estimated_size_1)) as i64,
                zstd_size: (actual_size_2.wrapping_sub(actual_size_1)) as i64,
                original_size: (group2_bytes.len().wrapping_sub(group1_bytes.len())) as i64,
            },
            group1_field_metrics,
            group2_field_metrics,
        });
    }
    split_comparisons
}

/// The result of comparing 2 arbitrary groups of fields based on the schema.
#[derive(Clone)]
pub struct SplitComparisonResult {
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
    /// The size difference between the two groups.
    pub group1_field_metrics: Vec<FieldMetrics>,
    /// The size difference between the two groups.
    pub group2_field_metrics: Vec<FieldMetrics>,
}

impl SplitComparisonResult {
    /// Generates a name from all field metrics in group 1
    pub fn group1_name(&self) -> String {
        self.group1_field_metrics
            .iter()
            .map(|m| m.name.as_str())
            .collect::<Vec<&str>>()
            .join(", ")
    }

    /// Generates a name from all field metrics in group 2
    pub fn group2_name(&self) -> String {
        self.group2_field_metrics
            .iter()
            .map(|m| m.name.as_str())
            .collect::<Vec<&str>>()
            .join(", ")
    }
}

/// The metrics for a group of fields.
#[derive(Clone, Default)]
pub struct GroupComparisonMetrics {
    /// Number of total LZ matches
    pub lz_matches: u64,
    /// Amount of entropy in the input data set
    pub entropy: f64,
    /// Size estimated by the size estimator function.
    pub estimated_size: u64,
    /// Size compressed by zstd.
    pub zstd_size: u64,
    /// Size of the original data.
    pub original_size: u64,
}

#[derive(Clone, Default)]
pub struct GroupDifference {
    /// The difference in LZ matches.
    pub lz_matches: i64,
    /// The difference in entropy
    pub entropy: f64,
    /// Difference in estimated size
    pub estimated_size: i64,
    /// Difference in zstd size
    pub zstd_size: i64,
    /// Difference in original size
    pub original_size: i64,
}
