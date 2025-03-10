use super::{
    analysis_results::AnalysisResults, print_field_metrics_bit_stats,
    print_field_metrics_value_stats, AnalysisMergeError, FieldMetrics, PrintFormat,
};
use crate::{
    comparison::{
        compare_groups::GroupComparisonResult,
        split_comparison::{
            calculate_max_entropy_diff, calculate_max_entropy_diff_ratio, FieldComparisonMetrics,
            SplitComparisonResult,
        },
        stats::{calculate_custom_zstd_ratio_stats, calculate_zstd_ratio_stats, format_stats},
        GroupComparisonMetrics, GroupDifference,
    },
    results::calculate_percentage,
    schema::{Metadata, Schema},
};
use ahash::{AHashMap, RandomState};
use rayon::prelude::*;
use std::collections::HashMap;
use std::io::{self, Write};

/// A struct that holds the aggregated results of multiple `AnalysisResults` instances.
/// It contains the same fields as `AnalysisResults` but represents the merged data
/// from multiple analyses. This is useful for analyzing results across multiple files
/// or data instances to identify patterns and trends.
#[derive(Clone, Default)]
pub struct MergedAnalysisResults {
    /// Schema metadata
    pub schema_metadata: Metadata,

    /// Average entropy of the merged files
    pub file_entropy: f64,

    /// Average LZ compression matches in the merged files
    pub file_lz_matches: u64,

    /// Average actual size of the compressed data when compressed with zstandard
    pub zstd_file_size: u64,

    /// Average original size of the uncompressed data
    pub original_size: u64,

    /// Total number of files that were merged
    pub merged_file_count: usize,

    /// Field path → computed metrics (merged)
    /// Maps each field's full path to the merged metrics across all analyzed files
    pub per_field: AHashMap<String, FieldMetrics>,

    /// Merged split comparison results
    pub split_comparisons: Vec<MergedSplitComparisonResult>,

    /// Merged custom group comparison results from schema-defined comparisons
    pub custom_comparisons: Vec<MergedGroupComparisonResult>,

    /// Original analysis results used to create this merged result.
    /// This is used for calculating statistics across the individual results.
    pub original_results: Vec<AnalysisResults>,
}

/// The result of comparing 2 arbitrary groups of fields based on the schema,
/// specifically for merged analysis results.
///
/// This is similar to [`SplitComparisonResult`] but includes additional information
/// related to statistics over multiple files.
#[derive(Clone, Default)]
pub struct MergedSplitComparisonResult {
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
    /// The statistics for the individual fields of the baseline group.
    pub baseline_comparison_metrics: Vec<FieldComparisonMetrics>,
    /// The statistics for the individual fields of the split group.
    pub split_comparison_metrics: Vec<FieldComparisonMetrics>,
    /// Ratio of how often the estimates and zstd sizes agree on which
    /// group compresses better.
    pub group_estimate_zstd_agreement_percentage: f64,
    /// Percentage of false positives: cases where the estimator predicted an improvement
    /// (group 2 better than group 1) but the actual zstd compression showed no improvement.
    pub group_estimate_false_positive_percentage: f64,
    /// Percentage of correct positives: cases where the estimator predicted an improvement
    /// (group 2 better than group 1) and the actual zstd compression confirmed this improvement.
    pub group_estimate_correct_positive_percentage: f64,
}

/// Contains the merged results of comparing custom field groupings defined in the schema.
/// This extends [`GroupComparisonResult`] with additional metrics that are calculated when merging multiple results.
#[derive(Clone)]
pub struct MergedGroupComparisonResult {
    /// The name of the group comparison. (Copied from schema)
    pub name: String,
    /// A description of the group comparison. (Copied from schema)
    pub description: String,
    /// Metrics for the baseline group.
    pub baseline_metrics: GroupComparisonMetrics,
    /// Names of the comparison groups in order they were specified in the schema
    pub group_names: Vec<String>,
    /// Metrics for the comparison groups in schema order
    pub group_metrics: Vec<GroupComparisonMetrics>,
    /// Comparison between other groups and first (baseline) group.
    pub differences: Vec<GroupDifference>,
    /// Percentage of times that the estimate agrees with zstd about which group (including baseline) has the smallest size
    pub estimate_zstd_agreement_percentage: f64,
}

impl MergedAnalysisResults {
    /// Create a new [`MergedAnalysisResults`] instance from a single [`AnalysisResults`].
    /// This serves as the starting point for merging multiple result sets.
    pub fn new(results: &AnalysisResults) -> Self {
        MergedAnalysisResults {
            schema_metadata: results.schema_metadata.clone(),
            file_entropy: results.file_entropy,
            file_lz_matches: results.file_lz_matches,
            zstd_file_size: results.zstd_file_size,
            original_size: results.original_size,
            merged_file_count: 1,
            per_field: results.per_field.clone(),
            split_comparisons: MergedSplitComparisonResult::from_split_comparisons(
                &results.split_comparisons,
            ),
            custom_comparisons: MergedGroupComparisonResult::from_group_comparisons(
                &results.custom_comparisons,
            ),
            original_results: vec![results.clone()],
        }
    }

    /// Create a new [`MergedAnalysisResults`] by merging multiple [`AnalysisResults`] instances.
    /// This efficiently processes all results in a single operation rather than
    /// incrementally merging them one by one.
    pub fn from_results(results: &[AnalysisResults]) -> Result<Self, AnalysisMergeError> {
        merge_analysis_results(results)
    }

    /// Convert the merged file statistics into a `FieldMetrics` object for comparisons
    pub fn as_field_metrics(&self) -> FieldMetrics {
        FieldMetrics {
            name: String::new(),
            full_path: String::new(),
            depth: 0,
            zstd_size: self.zstd_file_size,
            original_size: self.original_size,
            count: 0,
            lenbits: 0,
            entropy: self.file_entropy,
            lz_matches: self.file_lz_matches,
            bit_counts: Vec::new(),
            bit_order: crate::schema::BitOrder::Default,
            value_counts: rustc_hash::FxHashMap::default(),
        }
    }

    /// Print the merged analysis results
    pub fn print<W: Write>(
        &self,
        writer: &mut W,
        schema: &Schema,
        format: PrintFormat,
        skip_misc_stats: bool,
    ) -> io::Result<()> {
        writeln!(writer, "Aggregated (Merged) Analysis Results:")?;
        writeln!(writer, "Total files merged: {}", self.merged_file_count)?;

        match format {
            PrintFormat::Detailed => {
                self.print_detailed(writer, schema, &self.as_field_metrics(), skip_misc_stats)
            }
            PrintFormat::Concise => {
                self.print_concise(writer, schema, &self.as_field_metrics(), skip_misc_stats)
            }
        }
    }

    /// Print detailed format of the merged results
    fn print_detailed<W: Write>(
        &self,
        writer: &mut W,
        schema: &Schema,
        file_metrics: &FieldMetrics,
        skip_misc_stats: bool,
    ) -> io::Result<()> {
        writeln!(writer, "Schema: {}", self.schema_metadata.name)?;
        writeln!(writer, "Description: {}", self.schema_metadata.description)?;
        writeln!(writer, "File Entropy: {:.2} bits", self.file_entropy)?;
        writeln!(writer, "File LZ Matches: {}", self.file_lz_matches)?;
        writeln!(writer, "File Original Size: {}", self.original_size)?;
        writeln!(writer, "File Compressed Size: {}", self.zstd_file_size)?;
        writeln!(writer, "\nPer-field Metrics (in schema order):")?;

        // Iterate through schema-defined fields in order
        for field_path in schema.ordered_field_and_group_paths() {
            self.detailed_print_field(writer, file_metrics, &field_path)?;
        }

        writeln!(writer, "\nSplit Group Comparisons:")?;
        for comparison in &self.split_comparisons {
            self.detailed_print_comparison(writer, comparison)?;
        }

        writeln!(writer, "\nCustom Group Comparisons:")?;
        for comparison in &self.custom_comparisons {
            self.concise_print_custom_comparison(writer, comparison)?;
        }

        if !skip_misc_stats {
            writeln!(writer, "\nField Value Stats: [as `value: probability %`]")?;
            for field_path in schema.ordered_field_and_group_paths() {
                self.concise_print_field_value_stats(writer, &field_path)?;
            }

            writeln!(
                writer,
                "\nField Bit Stats: [as `(zeros/ones) (percentage %)`]"
            )?;
            for field_path in schema.ordered_field_and_group_paths() {
                self.concise_print_field_bit_stats(writer, &field_path)?;
            }
        }

        Ok(())
    }

    /// Print concise format of the merged results
    fn print_concise<W: Write>(
        &self,
        writer: &mut W,
        schema: &Schema,
        file_metrics: &FieldMetrics,
        skip_misc_stats: bool,
    ) -> io::Result<()> {
        writeln!(writer, "Schema: {}", self.schema_metadata.name)?;
        writeln!(
            writer,
            "File: {:.2}bpb, {} LZ, {}/{} ({:.2}%/{:.2}%) (zstd/orig)",
            self.file_entropy,
            self.file_lz_matches,
            self.zstd_file_size,
            self.original_size,
            calculate_percentage(self.zstd_file_size as f64, self.original_size as f64),
            100.0
        )?;

        writeln!(writer, "\nField Metrics:")?;
        for field_path in schema.ordered_field_and_group_paths() {
            self.concise_print_field(writer, file_metrics, &field_path)?;
        }

        writeln!(writer, "\nSplit Group Comparisons:")?;
        for comparison in &self.split_comparisons {
            self.concise_print_split_comparison(writer, comparison)?;
        }

        writeln!(writer, "\nCustom Group Comparisons:")?;
        for comparison in &self.custom_comparisons {
            self.concise_print_custom_comparison(writer, comparison)?;
        }

        if !skip_misc_stats {
            writeln!(writer, "\nField Value Stats: [as `value: probability %`]")?;
            for field_path in schema.ordered_field_and_group_paths() {
                self.concise_print_field_value_stats(writer, &field_path)?;
            }

            writeln!(
                writer,
                "\nField Bit Stats: [as `(zeros/ones) (percentage %)`]"
            )?;
            for field_path in schema.ordered_field_and_group_paths() {
                self.concise_print_field_bit_stats(writer, &field_path)?;
            }
        }

        Ok(())
    }

    // Helper methods for printing fields
    fn detailed_print_field<W: Write>(
        &self,
        writer: &mut W,
        file_metrics: &FieldMetrics,
        field_path: &str,
    ) -> io::Result<()> {
        if let Some(field) = self.per_field.get(field_path) {
            // Indent based on field depth to show hierarchy
            let indent = "  ".repeat(field.depth);
            let parent_stats = field.parent_metrics_in_merged_or(self, file_metrics);

            // Calculate percentages
            writeln!(
                writer,
                "{}{}: {:.2} bit entropy, {} LZ 3 Byte matches ({:.2}%)",
                indent,
                field.name,
                field.entropy,
                field.lz_matches,
                calculate_percentage(field.lz_matches as f64, parent_stats.lz_matches as f64)
            )?;
            let padding = format!("{}{}", indent, field.name).len() + 2; // +2 for ": "
            writeln!(
                writer,
                "{:padding$}Sizes: ZStandard -16/Original: {}/{} ({:.2}%/{:.2}%)",
                "",
                field.zstd_size,
                field.original_size,
                calculate_percentage(field.zstd_size as f64, parent_stats.zstd_size as f64),
                calculate_percentage(
                    field.original_size as f64,
                    parent_stats.original_size as f64
                )
            )?;
            writeln!(
                writer,
                "{:padding$}{} bit, {} unique values, {:?}",
                "",
                field.lenbits,
                field.value_counts.len(),
                field.bit_order
            )?;
        }

        Ok(())
    }

    fn concise_print_field<W: Write>(
        &self,
        writer: &mut W,
        file_metrics: &FieldMetrics,
        field_path: &str,
    ) -> io::Result<()> {
        if let Some(field) = self.per_field.get(field_path) {
            let indent = "  ".repeat(field.depth);
            let parent_stats = field.parent_metrics_in_merged_or(self, file_metrics);

            writeln!(
                writer,
                "{}{}: {:.2}bpb, {} LZ ({:.2}%), {}/{} ({:.2}%/{:.2}%) (zstd/orig), {}bit",
                indent,
                field.name,
                field.entropy,
                field.lz_matches,
                calculate_percentage(field.lz_matches as f64, parent_stats.lz_matches as f64),
                field.zstd_size,
                field.original_size,
                calculate_percentage(field.zstd_size as f64, parent_stats.zstd_size as f64),
                calculate_percentage(
                    field.original_size as f64,
                    parent_stats.original_size as f64
                ),
                field.lenbits
            )?;
        }

        Ok(())
    }

    fn concise_print_field_value_stats<W: Write>(
        &self,
        writer: &mut W,
        field_path: &str,
    ) -> io::Result<()> {
        if let Some(field) = self.per_field.get(field_path) {
            print_field_metrics_value_stats(writer, field)?;
        }

        Ok(())
    }

    fn concise_print_field_bit_stats<W: Write>(
        &self,
        writer: &mut W,
        field_path: &str,
    ) -> io::Result<()> {
        if let Some(field) = self.per_field.get(field_path) {
            print_field_metrics_bit_stats(writer, field)?;
        }

        Ok(())
    }

    fn detailed_print_comparison<W: Write>(
        &self,
        writer: &mut W,
        comparison: &MergedSplitComparisonResult,
    ) -> io::Result<()> {
        self.concise_print_split_comparison(writer, comparison)
    }

    fn concise_print_split_comparison<W: Write>(
        &self,
        writer: &mut W,
        comparison: &MergedSplitComparisonResult,
    ) -> io::Result<()> {
        let base_lz = comparison.group1_metrics.lz_matches;
        let size_orig = comparison.group1_metrics.original_size;
        let size_comp = comparison.group2_metrics.original_size;
        let base_entropy = comparison.group1_metrics.entropy;

        let base_zstd = comparison.group1_metrics.zstd_size;
        let base_estimated = comparison.group1_metrics.estimated_size;

        let comp_lz = comparison.group2_metrics.lz_matches;
        let comp_entropy = comparison.group2_metrics.entropy;

        let comp_zstd = comparison.group2_metrics.zstd_size;
        let comp_estimated = comparison.group2_metrics.estimated_size;

        let ratio_zstd = calculate_percentage(comp_zstd as f64, base_zstd as f64);
        let diff_zstd = comparison.difference.zstd_size;

        writeln!(writer, "  {}: {}", comparison.name, comparison.description)?;
        writeln!(writer, "    Original Size: {}", size_orig)?;
        writeln!(
            writer,
            "    Base LZ, Entropy: ({}, {:.2})",
            base_lz, base_entropy
        )?;
        writeln!(
            writer,
            "    Comp LZ, Entropy: ({}, {:.2})",
            comp_lz, comp_entropy
        )?;
        writeln!(
            writer,
            "    Base Group LZ, Entropy: ({:?}, {:?})",
            comparison
                .baseline_comparison_metrics
                .iter()
                .map(|m| m.lz_matches)
                .collect::<Vec<_>>(),
            comparison
                .baseline_comparison_metrics
                .iter()
                .map(|m| format!("{:.2}", m.entropy))
                .collect::<Vec<_>>()
        )?;
        writeln!(
            writer,
            "    Comp Group LZ, Entropy: ({:?}, {:?})",
            comparison
                .split_comparison_metrics
                .iter()
                .map(|m| m.lz_matches)
                .collect::<Vec<_>>(),
            comparison
                .split_comparison_metrics
                .iter()
                .map(|m| format!("{:.2}", m.entropy))
                .collect::<Vec<_>>()
        )?;

        if base_estimated != 0 {
            writeln!(
                writer,
                "    Base (est/zstd): {}/{}",
                base_estimated, base_zstd
            )?;
        } else {
            writeln!(writer, "    Base (zstd): {}", base_zstd)?;
        }

        if comp_estimated != 0 {
            writeln!(
                writer,
                "    Comp (est/zstd): {}/{}",
                comp_estimated, comp_zstd
            )?;
        } else {
            writeln!(writer, "    Comp (zstd): {}", comp_zstd)?;
        }

        writeln!(writer, "    Ratio (zstd): {}", ratio_zstd)?;
        writeln!(writer, "    Diff (zstd): {}", diff_zstd)?;
        writeln!(
            writer,
            "    Est/Zstd Agreement on Better Group: {:.1}%",
            comparison.group_estimate_zstd_agreement_percentage
        )?;
        writeln!(
            writer,
            "    Est/Zstd False Positives: {:.1}%",
            comparison.group_estimate_false_positive_percentage
        )?;
        writeln!(
            writer,
            "    Est/Zstd Correct Positives: {:.1}%",
            comparison.group_estimate_correct_positive_percentage
        )?;

        // If we have enough files for statistics, show the detailed stats
        writeln!(writer, "    Zstd Ratio Statistics:")?;

        // Find the index of this comparison in the split_comparisons array
        let comp_index = self
            .split_comparisons
            .iter()
            .position(|c| c.name == comparison.name)
            .unwrap_or(0);

        // Calculate and print the zstd ratio statistics
        if let Some(stats) = calculate_zstd_ratio_stats(&self.original_results, comp_index) {
            writeln!(writer, "    * {}", format_stats(&stats))?;
        } else {
            writeln!(writer, "    * No statistics available (insufficient data)")?;
        }

        if size_orig != size_comp {
            writeln!(writer, "    [WARNING!!] Sizes of both groups in bytes don't match!! They may vary by a few bytes due to padding.")?;
            writeln!(writer, "    [WARNING!!] However if they vary extremely, your groups may be incorrect. group1: {}, group2: {}", size_orig, size_comp)?;
        }

        Ok(())
    }

    fn concise_print_custom_comparison<W: Write>(
        &self,
        writer: &mut W,
        comparison: &MergedGroupComparisonResult,
    ) -> io::Result<()> {
        let base_lz = comparison.baseline_metrics.lz_matches;
        let base_entropy = comparison.baseline_metrics.entropy;
        let base_zstd = comparison.baseline_metrics.zstd_size;
        let base_estimated = comparison.baseline_metrics.estimated_size;
        let base_size = comparison.baseline_metrics.original_size;

        writeln!(writer, "  {}: {}", comparison.name, comparison.description)?;
        writeln!(
            writer,
            "    Overall Est/Zstd Agreement on Best Group: {:.1}%",
            comparison.estimate_zstd_agreement_percentage * 100.0
        )?;
        writeln!(writer, "    Base Group:")?;
        writeln!(writer, "      Size: {}", base_size)?;
        writeln!(
            writer,
            "      LZ, Entropy: ({}, {:.2})",
            base_lz, base_entropy
        )?;
        if base_estimated != 0 {
            writeln!(
                writer,
                "      Base (est/zstd): {}/{}",
                base_estimated, base_zstd
            )?;
        } else {
            writeln!(writer, "      Base (zstd): {}", base_zstd)?;
        }

        for (x, (group_name, metrics)) in comparison
            .group_names
            .iter()
            .zip(&comparison.group_metrics)
            .enumerate()
        {
            let comp_lz = metrics.lz_matches;
            let comp_entropy = metrics.entropy;
            let comp_zstd = metrics.zstd_size;
            let comp_estimated = metrics.estimated_size;
            let comp_size = metrics.original_size;

            let ratio_zstd = calculate_percentage(comp_zstd as f64, base_zstd as f64);
            let diff_zstd = comparison.differences[x].zstd_size;

            writeln!(writer, "\n    {} Group:", group_name)?;
            writeln!(writer, "      Size: {}", comp_size)?;
            writeln!(
                writer,
                "      LZ, Entropy: ({}, {:.2})",
                comp_lz, comp_entropy
            )?;
            if comp_estimated != 0 {
                writeln!(
                    writer,
                    "      Comp (est/zstd): {}/{}",
                    comp_estimated, comp_zstd
                )?;
            } else {
                writeln!(writer, "      Comp (zstd): {}", comp_zstd)?;
            }
            writeln!(writer, "      Ratio (zstd): {:.1}%", ratio_zstd)?;
            writeln!(writer, "      Diff (zstd): {}", diff_zstd)?;

            // Find the index of this comparison in the custom_comparisons array
            if let Some(comp_index) = self
                .custom_comparisons
                .iter()
                .position(|c| c.name == comparison.name)
            {
                // Calculate and print the zstd ratio statistics for this group
                if let Some(stats) =
                    calculate_custom_zstd_ratio_stats(&self.original_results, comp_index, x)
                {
                    writeln!(writer, "      Zstd Ratio Statistics:")?;
                    writeln!(writer, "      * {}", format_stats(&stats))?;
                }
            }

            if base_size != comp_size {
                writeln!(writer, "      [WARNING!!] Sizes of base and comparison groups don't match!! They may vary by a few bytes due to padding.")?;
                writeln!(writer, "      [WARNING!!] However if they vary extremely, your groups may be incorrect. base: {}, {}: {}", base_size, group_name, comp_size)?;
            }
        }

        Ok(())
    }
}

/// Helper functions around [`MergedSplitComparisonResult`]
impl MergedSplitComparisonResult {
    /// Create a new [`MergedSplitComparisonResult`] from a [`SplitComparisonResult`]
    pub fn from_split_comparison(result: &SplitComparisonResult) -> Self {
        Self {
            name: result.name.clone(),
            description: result.description.clone(),
            group1_metrics: result.group1_metrics,
            group2_metrics: result.group2_metrics,
            difference: result.difference,
            baseline_comparison_metrics: result.baseline_comparison_metrics.clone(),
            split_comparison_metrics: result.split_comparison_metrics.clone(),
            group_estimate_zstd_agreement_percentage: 0.0,
            group_estimate_false_positive_percentage: 0.0,
            group_estimate_correct_positive_percentage: 0.0,
        }
    }

    /// Convert a Vec of SplitComparisonResult to a Vec of MergedSplitComparisonResult
    pub fn from_split_comparisons(results: &[SplitComparisonResult]) -> Vec<Self> {
        results.iter().map(Self::from_split_comparison).collect()
    }

    /// Ratio between the max and min entropy of the baseline fields.
    pub fn baseline_max_entropy_diff_ratio(&self) -> f64 {
        calculate_max_entropy_diff_ratio(&self.baseline_comparison_metrics)
    }

    /// Maximum difference between the entropy of the baseline fields.
    pub fn baseline_max_entropy_diff(&self) -> f64 {
        calculate_max_entropy_diff(&self.baseline_comparison_metrics)
    }

    /// Maximum difference between the entropy of the split fields.
    pub fn split_max_entropy_diff(&self) -> f64 {
        calculate_max_entropy_diff(&self.split_comparison_metrics)
    }

    /// Ratio between the max and min entropy of the split fields.
    pub fn split_max_entropy_diff_ratio(&self) -> f64 {
        calculate_max_entropy_diff_ratio(&self.split_comparison_metrics)
    }

    /// Convert to a [`SplitComparisonResult`] (primarily for backward compatibility)
    pub fn to_split_comparison(&self) -> SplitComparisonResult {
        SplitComparisonResult {
            name: self.name.clone(),
            description: self.description.clone(),
            group1_metrics: self.group1_metrics,
            group2_metrics: self.group2_metrics,
            difference: self.difference,
            baseline_comparison_metrics: self.baseline_comparison_metrics.clone(),
            split_comparison_metrics: self.split_comparison_metrics.clone(),
        }
    }
}

impl MergedGroupComparisonResult {
    fn from_group_comparisons(
        custom_comparisons: &[GroupComparisonResult],
    ) -> Vec<MergedGroupComparisonResult> {
        custom_comparisons
            .iter()
            .map(Self::from_group_comparison)
            .collect()
    }

    fn from_group_comparison(comparison: &GroupComparisonResult) -> Self {
        MergedGroupComparisonResult {
            name: comparison.name.clone(),
            description: comparison.description.clone(),
            baseline_metrics: comparison.baseline_metrics,
            group_names: comparison.group_names.clone(),
            group_metrics: comparison.group_metrics.clone(),
            differences: comparison.differences.clone(),
            estimate_zstd_agreement_percentage: 0.0,
        }
    }
}

/// Create a new [`MergedAnalysisResults`] by merging multiple [`AnalysisResults`] instances.
/// This efficiently processes all results in a single operation rather than
/// incrementally merging them one by one.
pub fn merge_analysis_results(
    results: &[AnalysisResults],
) -> Result<MergedAnalysisResults, AnalysisMergeError> {
    let mut merged = MergedAnalysisResults::default();
    if results.is_empty() {
        return Ok(merged);
    }

    // Calculate average of each field.
    let total_count = results.len();
    let mut total_entropy = 0_f64;
    let mut total_lz_matches = 0;
    let mut total_zstd_size = 0;
    let mut total_original_size = 0;

    for result in results {
        total_entropy += result.file_entropy;
        total_lz_matches += result.file_lz_matches;
        total_zstd_size += result.zstd_file_size;
        total_original_size += result.original_size;
    }

    merged.file_entropy = total_entropy / total_count as f64;
    merged.file_lz_matches = total_lz_matches / total_count as u64;
    merged.zstd_file_size = total_zstd_size / total_count as u64;
    merged.original_size = total_original_size / total_count as u64;
    merged.merged_file_count = total_count;

    // Merge field-level metrics in parallel
    let first = &results[0];
    merged.schema_metadata = first.schema_metadata.clone();

    merged.per_field = first
        .per_field
        .par_iter()
        .map(|(full_path, _)| {
            // Get all matching `full_path` from all other elements as vec
            let metrics_for_field: Vec<&FieldMetrics> = results
                .iter()
                .flat_map(|results| results.per_field.get(full_path))
                .collect();

            // Return merged FieldMetrics, or error.
            FieldMetrics::try_merge_many(&metrics_for_field)
                .map(|merged| (full_path.clone(), merged))
        })
        // Convert into HashMap. Need to explicitly set inner AHashMap type, because AHashMap not supported.
        .collect::<Result<HashMap<String, FieldMetrics, RandomState>, _>>()?
        .into();

    // Merge split comparisons
    merged.split_comparisons = merge_split_comparisons(results);
    merged.custom_comparisons = merge_custom_comparisons(results);
    merged.original_results = results.to_vec();
    Ok(merged)
}

fn merge_split_comparisons(items: &[AnalysisResults]) -> Vec<MergedSplitComparisonResult> {
    if items.is_empty() || items[0].split_comparisons.is_empty() {
        return Vec::new();
    }

    // Create vector to hold results
    let comparisons_count = items[0].split_comparisons.len();
    let mut merged_comparisons = Vec::with_capacity(comparisons_count);

    // For each comparison in the first result...
    for x in 0..comparisons_count {
        merged_comparisons.push(merge_split_comparison(x, items));
    }

    merged_comparisons
}

fn merge_split_comparison(
    split_idx: usize,
    items: &[AnalysisResults],
) -> MergedSplitComparisonResult {
    let mut merged = MergedSplitComparisonResult {
        name: items[0].split_comparisons[split_idx].name.clone(),
        description: items[0].split_comparisons[split_idx].description.clone(),
        group1_metrics: GroupComparisonMetrics::default(),
        group2_metrics: GroupComparisonMetrics::default(),
        difference: GroupDifference::default(),
        baseline_comparison_metrics: Vec::new(),
        split_comparison_metrics: Vec::new(),
        group_estimate_zstd_agreement_percentage: 0.0,
        group_estimate_false_positive_percentage: 0.0,
        group_estimate_correct_positive_percentage: 0.0,
    };

    // First calculate G1 metrics
    let g1_metrics = &mut merged.group1_metrics;
    for item in items {
        g1_metrics.lz_matches += item.split_comparisons[split_idx].group1_metrics.lz_matches;
        g1_metrics.entropy += item.split_comparisons[split_idx].group1_metrics.entropy;
        g1_metrics.estimated_size += item.split_comparisons[split_idx]
            .group1_metrics
            .estimated_size;
        g1_metrics.zstd_size += item.split_comparisons[split_idx].group1_metrics.zstd_size;
        g1_metrics.original_size += item.split_comparisons[split_idx]
            .group1_metrics
            .original_size;
    }
    g1_metrics.lz_matches /= items.len() as u64;
    g1_metrics.entropy /= items.len() as f64;
    g1_metrics.estimated_size /= items.len() as u64;
    g1_metrics.zstd_size /= items.len() as u64;
    g1_metrics.original_size /= items.len() as u64;

    // Second calculate G2 metrics
    let g2_metrics = &mut merged.group2_metrics;
    for item in items {
        g2_metrics.lz_matches += item.split_comparisons[split_idx].group2_metrics.lz_matches;
        g2_metrics.entropy += item.split_comparisons[split_idx].group2_metrics.entropy;
        g2_metrics.estimated_size += item.split_comparisons[split_idx]
            .group2_metrics
            .estimated_size;
        g2_metrics.zstd_size += item.split_comparisons[split_idx].group2_metrics.zstd_size;
        g2_metrics.original_size += item.split_comparisons[split_idx]
            .group2_metrics
            .original_size;
    }
    g2_metrics.lz_matches /= items.len() as u64;
    g2_metrics.entropy /= items.len() as f64;
    g2_metrics.estimated_size /= items.len() as u64;
    g2_metrics.zstd_size /= items.len() as u64;
    g2_metrics.original_size /= items.len() as u64;

    // Calculate agreement percentage between zstd and estimate
    // on which group compresses better.
    let mut agreement_count = 0;
    let mut total_count = 0;
    let mut false_positive_count = 0;
    let mut correct_positive_count = 0;
    for item in items {
        let g1 = &item.split_comparisons[split_idx].group1_metrics;
        let g2 = &item.split_comparisons[split_idx].group2_metrics;
        if g1.estimated_size != 0 && g2.estimated_size != 0 {
            total_count += 1;
            let est_g2_better = g2.estimated_size < g1.estimated_size;
            let zstd_g2_better = g2.zstd_size < g1.zstd_size;
            if est_g2_better == zstd_g2_better {
                agreement_count += 1;
            }

            // Count false positives: estimator thinks group 2 is better, but it's not
            if est_g2_better && !zstd_g2_better {
                false_positive_count += 1;
            }

            // Count correct positives: estimator thinks group 2 is better, and it is
            if est_g2_better && zstd_g2_better {
                correct_positive_count += 1;
            }
        }
    }

    merged.group_estimate_zstd_agreement_percentage = if total_count > 0 {
        (agreement_count as f64 / total_count as f64) * 100.0
    } else {
        0.0
    };

    merged.group_estimate_false_positive_percentage = if total_count > 0 {
        (false_positive_count as f64 / total_count as f64) * 100.0
    } else {
        0.0
    };

    merged.group_estimate_correct_positive_percentage = if total_count > 0 {
        (correct_positive_count as f64 / total_count as f64) * 100.0
    } else {
        0.0
    };

    // Now calculate difference
    let difference = &mut merged.difference;
    for item in items {
        difference.lz_matches += item.split_comparisons[split_idx].difference.lz_matches;
        difference.entropy += item.split_comparisons[split_idx].difference.entropy;
        difference.estimated_size += item.split_comparisons[split_idx].difference.estimated_size;
        difference.zstd_size += item.split_comparisons[split_idx].difference.zstd_size;
        difference.original_size += item.split_comparisons[split_idx].difference.original_size;
    }
    difference.lz_matches /= items.len() as i64;
    difference.entropy /= items.len() as f64;
    difference.estimated_size /= items.len() as i64;
    difference.zstd_size /= items.len() as i64;
    difference.original_size /= items.len() as i64;

    // Merge baseline metrics
    let mut baseline_metrics =
        vec![GroupComparisonMetrics::default(); items[0].split_comparisons.len()];
    for (index, merged) in baseline_metrics.iter_mut().enumerate() {
        for item in items {
            merged.lz_matches = item.split_comparisons[index].group1_metrics.lz_matches;
            merged.entropy = item.split_comparisons[index].group1_metrics.entropy;
            merged.estimated_size = item.split_comparisons[index].group1_metrics.estimated_size;
            merged.zstd_size = item.split_comparisons[index].group1_metrics.zstd_size;
            merged.original_size = item.split_comparisons[index].group1_metrics.original_size;
        }

        merged.lz_matches /= items.len() as u64;
        merged.entropy /= items.len() as f64;
        merged.estimated_size /= items.len() as u64;
        merged.zstd_size /= items.len() as u64;
        merged.original_size /= items.len() as u64;
    }

    // Merge split metrics
    let mut split_metrics =
        vec![GroupComparisonMetrics::default(); items[0].split_comparisons.len()];
    for (index, merged) in split_metrics.iter_mut().enumerate() {
        for item in items {
            merged.lz_matches = item.split_comparisons[index].group2_metrics.lz_matches;
            merged.entropy = item.split_comparisons[index].group2_metrics.entropy;
            merged.estimated_size = item.split_comparisons[index].group2_metrics.estimated_size;
            merged.zstd_size = item.split_comparisons[index].group2_metrics.zstd_size;
            merged.original_size = item.split_comparisons[index].group2_metrics.original_size;
        }

        merged.lz_matches /= items.len() as u64;
        merged.entropy /= items.len() as f64;
        merged.estimated_size /= items.len() as u64;
        merged.zstd_size /= items.len() as u64;
        merged.original_size /= items.len() as u64;
    }

    // Update 'baseline_comparison_metrics'
    let baseline_metrics = &items[0].split_comparisons[split_idx].baseline_comparison_metrics;
    if !baseline_metrics.is_empty() {
        // Initialize merged metrics with default values
        let field_count = baseline_metrics.len();
        merged.baseline_comparison_metrics = vec![FieldComparisonMetrics::default(); field_count];

        // Sum up metrics from all items
        for item in items {
            for (x, field_metrics) in item.split_comparisons[split_idx]
                .baseline_comparison_metrics
                .iter()
                .enumerate()
            {
                merged.baseline_comparison_metrics[x].lz_matches += field_metrics.lz_matches;
                merged.baseline_comparison_metrics[x].entropy += field_metrics.entropy;
            }
        }

        // Calculate averages
        for field_metrics in &mut merged.baseline_comparison_metrics {
            field_metrics.lz_matches /= items.len() as u64;
            field_metrics.entropy /= items.len() as f64;
        }
    }

    // Update 'split_comparison_metrics'
    let split_metrics = &items[0].split_comparisons[split_idx].split_comparison_metrics;
    if !split_metrics.is_empty() {
        // Initialize merged metrics with default values
        let field_count = split_metrics.len();
        merged.split_comparison_metrics = vec![FieldComparisonMetrics::default(); field_count];

        // Sum up metrics from all items
        for item in items {
            for (x, field_metrics) in item.split_comparisons[split_idx]
                .split_comparison_metrics
                .iter()
                .enumerate()
            {
                merged.split_comparison_metrics[x].lz_matches += field_metrics.lz_matches;
                merged.split_comparison_metrics[x].entropy += field_metrics.entropy;
            }
        }

        // Calculate averages
        for field_metrics in &mut merged.split_comparison_metrics {
            field_metrics.lz_matches /= items.len() as u64;
            field_metrics.entropy /= items.len() as f64;
        }
    }

    merged
}

fn merge_custom_comparisons(items: &[AnalysisResults]) -> Vec<MergedGroupComparisonResult> {
    if items.is_empty() {
        return Vec::new();
    }

    let comparisons_count = items[0].custom_comparisons.len();
    let mut merged_comparisons = Vec::with_capacity(comparisons_count);

    for x in 0..comparisons_count {
        merged_comparisons.push(merge_custom_comparison(x, items));
    }

    merged_comparisons
}

fn merge_custom_comparison(index: usize, items: &[AnalysisResults]) -> MergedGroupComparisonResult {
    let mut merged = MergedGroupComparisonResult {
        name: items[0].custom_comparisons[index].name.clone(),
        description: items[0].custom_comparisons[index].description.clone(),
        baseline_metrics: GroupComparisonMetrics::default(),
        group_names: items[0].custom_comparisons[index].group_names.clone(),
        group_metrics: Vec::with_capacity(items[0].custom_comparisons[index].group_metrics.len()),
        differences: Vec::with_capacity(items[0].custom_comparisons[index].differences.len()),
        estimate_zstd_agreement_percentage: 0.0,
    };

    // Calculate merged baseline metrics
    let baseline_metrics = &mut merged.baseline_metrics;
    for item in items {
        baseline_metrics.lz_matches += item.custom_comparisons[index].baseline_metrics.lz_matches;
        baseline_metrics.entropy += item.custom_comparisons[index].baseline_metrics.entropy;
        baseline_metrics.estimated_size += item.custom_comparisons[index]
            .baseline_metrics
            .estimated_size;
        baseline_metrics.zstd_size += item.custom_comparisons[index].baseline_metrics.zstd_size;
        baseline_metrics.original_size += item.custom_comparisons[index]
            .baseline_metrics
            .original_size;
    }

    baseline_metrics.lz_matches /= items.len() as u64;
    baseline_metrics.entropy /= items.len() as f64;
    baseline_metrics.estimated_size /= items.len() as u64;
    baseline_metrics.zstd_size /= items.len() as u64;
    baseline_metrics.original_size /= items.len() as u64;

    // Calculate merged group metrics
    let group_count = items[0].custom_comparisons[index].group_metrics.len();
    merged.group_metrics = vec![GroupComparisonMetrics::default(); group_count];

    for (group_idx, merged_group_metrics) in merged.group_metrics.iter_mut().enumerate() {
        for item in items {
            merged_group_metrics.lz_matches +=
                item.custom_comparisons[index].group_metrics[group_idx].lz_matches;
            merged_group_metrics.entropy +=
                item.custom_comparisons[index].group_metrics[group_idx].entropy;
            merged_group_metrics.estimated_size +=
                item.custom_comparisons[index].group_metrics[group_idx].estimated_size;
            merged_group_metrics.zstd_size +=
                item.custom_comparisons[index].group_metrics[group_idx].zstd_size;
            merged_group_metrics.original_size +=
                item.custom_comparisons[index].group_metrics[group_idx].original_size;
        }

        merged_group_metrics.lz_matches /= items.len() as u64;
        merged_group_metrics.entropy /= items.len() as f64;
        merged_group_metrics.estimated_size /= items.len() as u64;
        merged_group_metrics.zstd_size /= items.len() as u64;
        merged_group_metrics.original_size /= items.len() as u64;
    }

    // Calculate merged differences
    let diff_count = items[0].custom_comparisons[index].differences.len();
    merged.differences = vec![GroupDifference::default(); diff_count];

    for (diff_idx, merged_diff) in merged.differences.iter_mut().enumerate() {
        for item in items {
            merged_diff.lz_matches +=
                item.custom_comparisons[index].differences[diff_idx].lz_matches;
            merged_diff.entropy += item.custom_comparisons[index].differences[diff_idx].entropy;
            merged_diff.estimated_size +=
                item.custom_comparisons[index].differences[diff_idx].estimated_size;
            merged_diff.zstd_size += item.custom_comparisons[index].differences[diff_idx].zstd_size;
            merged_diff.original_size +=
                item.custom_comparisons[index].differences[diff_idx].original_size;
        }
        merged_diff.lz_matches /= items.len() as i64;
        merged_diff.entropy /= items.len() as f64;
        merged_diff.estimated_size /= items.len() as i64;
        merged_diff.zstd_size /= items.len() as i64;
        merged_diff.original_size /= items.len() as i64;
    }

    // Calculate estimate/zstd agreement percentage
    // This measures how often our estimate correctly identifies the group with the smallest zstd size
    let mut agreement_count = 0;
    let mut total_count = 0;

    for item in items {
        // Skip if estimated sizes are not available
        if item.custom_comparisons[index]
            .baseline_metrics
            .estimated_size
            == 0
        {
            continue;
        }

        // Check if any group metrics are missing estimated sizes
        let mut missing_estimates = false;
        for group_metrics in &item.custom_comparisons[index].group_metrics {
            if group_metrics.estimated_size == 0 {
                missing_estimates = true;
                break;
            }
        }

        if missing_estimates {
            continue;
        }

        total_count += 1;

        // Find the group with the smallest zstd size (including baseline)
        let mut smallest_zstd_idx = -1; // -1 means baseline
        let mut smallest_zstd = item.custom_comparisons[index].baseline_metrics.zstd_size;

        for (x, group_metrics) in item.custom_comparisons[index]
            .group_metrics
            .iter()
            .enumerate()
        {
            if group_metrics.zstd_size < smallest_zstd {
                smallest_zstd = group_metrics.zstd_size;
                smallest_zstd_idx = x as i32;
            }
        }

        // Find the group with the smallest estimated size (including baseline)
        let mut smallest_est_idx = -1; // -1 means baseline
        let mut smallest_est = item.custom_comparisons[index]
            .baseline_metrics
            .estimated_size;

        for (x, group_metrics) in item.custom_comparisons[index]
            .group_metrics
            .iter()
            .enumerate()
        {
            if group_metrics.estimated_size < smallest_est {
                smallest_est = group_metrics.estimated_size;
                smallest_est_idx = x as i32;
            }
        }

        // Check if the estimates agree on which group is smallest
        if smallest_zstd_idx == smallest_est_idx {
            agreement_count += 1;
        }
    }

    merged.estimate_zstd_agreement_percentage = if total_count > 0 {
        agreement_count as f64 / total_count as f64
    } else {
        0.0
    };

    merged
}

impl From<GroupComparisonResult> for MergedGroupComparisonResult {
    fn from(result: GroupComparisonResult) -> Self {
        Self {
            name: result.name,
            description: result.description,
            baseline_metrics: result.baseline_metrics,
            group_names: result.group_names,
            group_metrics: result.group_metrics,
            differences: result.differences,
            estimate_zstd_agreement_percentage: 0.0,
        }
    }
}
