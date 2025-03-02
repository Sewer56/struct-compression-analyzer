use super::{
    print_field_metrics_bit_stats, print_field_metrics_value_stats, ComputeAnalysisResultsError,
    FieldMetrics, PrintFormat,
};
use crate::{
    analyzer::{AnalyzerFieldState, CompressionOptions, SchemaAnalyzer},
    comparison::{
        compare_groups::{analyze_custom_comparisons, GroupComparisonResult},
        split_comparison::{
            make_split_comparison_result, FieldComparisonMetrics, SplitComparisonResult,
        },
    },
    results::calculate_percentage,
    schema::{BitOrder, Metadata, Schema, SplitComparison},
    utils::analyze_utils::{calculate_file_entropy, get_writer_buffer, get_zstd_compressed_size},
};
use ahash::{AHashMap, HashMapExt};
use lossless_transform_utils::match_estimator::estimate_num_lz_matches_fast;
use rustc_hash::FxHashMap;

/// Final computed metrics for output
#[derive(Clone)]
pub struct AnalysisResults {
    /// Schema name
    pub schema_metadata: Metadata,

    /// Entropy of the whole file
    pub file_entropy: f64,

    /// LZ compression matches in the file
    pub file_lz_matches: usize,

    /// Actual size of the compressed data when compressed with zstandard
    pub zstd_file_size: usize,

    /// Original size of the uncompressed data
    pub original_size: usize,

    /// Field path → computed metrics
    /// This is a map of `full_path` to [`FieldMetrics`], such that we
    /// can easily merge the results of different fields down the road.
    pub per_field: AHashMap<String, FieldMetrics>,

    /// Split comparison results
    pub split_comparisons: Vec<SplitComparisonResult>,

    /// Custom group comparison results from schema-defined comparisons
    pub custom_comparisons: Vec<GroupComparisonResult>,
}

/// Given a [`SchemaAnalyzer`] which has ingested all of the data to be calculated, via
/// the [`SchemaAnalyzer::add_entry`] function, compute the analysis results.
///
/// This returns the results for all of the per-field metrics, as well as computing the
/// various schema defined groups, such as 'split' groups and 'compare' groups.
pub fn compute_analysis_results(
    analyzer: &mut SchemaAnalyzer,
) -> Result<AnalysisResults, ComputeAnalysisResultsError> {
    // First calculate file entropy
    let file_entropy = calculate_file_entropy(&analyzer.entries);
    let file_lz_matches = estimate_num_lz_matches_fast(&analyzer.entries);

    // Then calculate per-field entropy and lz matches
    let mut field_metrics: AHashMap<String, FieldMetrics> = AHashMap::new();

    for stats in &mut analyzer.field_states.values_mut() {
        let writer_buffer = get_writer_buffer(&mut stats.writer);
        let entropy = calculate_file_entropy(writer_buffer);
        let lz_matches = estimate_num_lz_matches_fast(writer_buffer);
        let actual_size = get_zstd_compressed_size(
            writer_buffer,
            analyzer.compression_options.zstd_compression_level,
        );

        // reduce memory usage from leftover analyzer.
        stats.value_counts.shrink_to_fit();
        field_metrics.insert(
            stats.full_path.clone(),
            FieldMetrics {
                name: stats.name.clone(),
                full_path: stats.full_path.clone(),
                entropy,
                lz_matches,
                bit_counts: stats.bit_counts.clone(),
                value_counts: stats.value_counts.clone(),
                depth: stats.depth,
                count: stats.count,
                lenbits: stats.lenbits,
                bit_order: stats.bit_order,
                zstd_size: actual_size,
                original_size: writer_buffer.len(),
            },
        );
    }

    // Process split group comparisons
    let split_comparisons = calc_split_comparisons(
        &mut analyzer.field_states,
        &analyzer.schema.analysis.split_groups,
        &field_metrics,
        analyzer.compression_options,
    );

    // Process custom group comparisons
    let custom_comparisons = analyze_custom_comparisons(
        analyzer.schema,
        &mut analyzer.field_states,
        analyzer.compression_options,
    )?;

    Ok(AnalysisResults {
        file_entropy,
        file_lz_matches,
        per_field: field_metrics,
        schema_metadata: analyzer.schema.metadata.clone(),
        zstd_file_size: get_zstd_compressed_size(
            &analyzer.entries,
            analyzer.compression_options.zstd_compression_level,
        ),
        original_size: analyzer.entries.len(),
        split_comparisons,
        custom_comparisons,
    })
}

/// Calculates the comparison results between a series of field splits.
///
/// This function takes the [`SchemaAnalyzer`]'s intermediate state, that is, the
/// state of each field (containing the data for each field), a list of split comparisons
/// to make, and the individual metrics (results) for each field.
///
/// This then computes the comparison results for each split.
///
/// # Remarks
/// This API is for internal use. It may change without notice.
///
/// # Arguments
/// * `field_stats` - The current field states (analyzer working state)
/// * `comparisons` - A slice of [`SplitComparison`] objects defining the splits to compare.
/// * `field_metrics` - A reference to a hash map of field metrics.
/// * `compression_options` - The compression options (zstd compression level, etc).
///
/// # Returns
/// A vector of [`SplitComparisonResult`] objects containing the comparison results.
///
/// [`SchemaAnalyzer`]: crate::analyzer::SchemaAnalyzer
fn calc_split_comparisons(
    field_stats: &mut AHashMap<String, AnalyzerFieldState>,
    comparisons: &[SplitComparison],
    field_metrics: &AHashMap<String, FieldMetrics>,
    compression_options: CompressionOptions,
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

        let mut group1_field_metrics: Vec<FieldComparisonMetrics> = Vec::new();
        let mut group2_field_metrics: Vec<FieldComparisonMetrics> = Vec::new();
        for path in &comparison.group_1 {
            if let Some(metrics) = field_metrics.iter().find(|(_k, v)| v.name == *path) {
                group1_field_metrics.push(metrics.1.clone().into());
            }
        }
        for path in &comparison.group_2 {
            if let Some(metrics) = field_metrics.iter().find(|(_k, v)| v.name == *path) {
                group2_field_metrics.push(metrics.1.clone().into());
            }
        }

        // Create custom compression options for this comparison using its multipliers
        let custom_compression_options = CompressionOptions {
            zstd_compression_level: compression_options.zstd_compression_level,
            size_estimator_fn: compression_options.size_estimator_fn,
            lz_match_multiplier: compression_options.lz_match_multiplier,
            entropy_multiplier: compression_options.entropy_multiplier,
        };

        split_comparisons.push(make_split_comparison_result(
            comparison.name.clone(),
            comparison.description.clone(),
            &group1_bytes,
            &group2_bytes,
            group1_field_metrics,
            group2_field_metrics,
            custom_compression_options,
            comparison.compression_estimation_group_1.clone(),
            comparison.compression_estimation_group_2.clone(),
        ));
    }
    split_comparisons
}

impl AnalysisResults {
    /// Converts the file level statistics into a [`FieldMetrics`] object
    /// which can be used for comparison with parent in places such as the
    /// print function.
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
            bit_order: BitOrder::Default,
            value_counts: FxHashMap::new(),
        }
    }

    pub fn print(&self, schema: &Schema, format: PrintFormat, skip_misc_stats: bool) {
        match format {
            PrintFormat::Detailed => {
                self.print_detailed(schema, &self.as_field_metrics(), skip_misc_stats)
            }
            PrintFormat::Concise => {
                self.print_concise(schema, &self.as_field_metrics(), skip_misc_stats)
            }
        }
    }

    fn print_detailed(&self, schema: &Schema, file_metrics: &FieldMetrics, skip_misc_stats: bool) {
        println!("Schema: {}", self.schema_metadata.name);
        println!("Description: {}", self.schema_metadata.description);
        println!("File Entropy: {:.2} bits", self.file_entropy);
        println!("File LZ Matches: {}", self.file_lz_matches);
        println!("File Original Size: {}", self.original_size);
        println!("File Compressed Size: {}", self.zstd_file_size);
        println!("\nPer-field Metrics (in schema order):");

        // Iterate through schema-defined fields in order
        for field_path in schema.ordered_field_and_group_paths() {
            self.detailed_print_field(file_metrics, &field_path);
        }

        println!("\nSplit Group Comparisons:");
        for comparison in &self.split_comparisons {
            detailed_print_comparison(comparison);
        }

        println!("\nCustom Group Comparisons:");
        for comparison in &self.custom_comparisons {
            concise_print_custom_comparison(comparison);
        }

        if !skip_misc_stats {
            println!("\nField Value Stats: [as `value: probability %`]");
            for field_path in schema.ordered_field_and_group_paths() {
                self.concise_print_field_value_stats(&field_path);
            }

            println!("\nField Bit Stats: [as `(zeros/ones) (percentage %)`]");
            for field_path in schema.ordered_field_and_group_paths() {
                self.concise_print_field_bit_stats(&field_path);
            }
        }
    }

    fn detailed_print_field(&self, file_metrics: &FieldMetrics, field_path: &str) {
        if let Some(field) = self.per_field.get(field_path) {
            // Indent based on field depth to show hierarchy
            let indent = "  ".repeat(field.depth);
            let parent_stats = field.parent_metrics_or(self, file_metrics);

            // Calculate percentages
            println!(
                "{}{}: {:.2} bit entropy, {} LZ 3 Byte matches ({:.2}%)",
                indent,
                field.name,
                field.entropy,
                field.lz_matches,
                calculate_percentage(field.lz_matches as f64, parent_stats.lz_matches as f64)
            );
            let padding = format!("{}{}", indent, field.name).len() + 2; // +2 for ": "
            println!(
                "{:padding$}Sizes: ZStandard/Original: {}/{} ({:.2}%/{:.2}%)",
                "",
                field.zstd_size,
                field.original_size,
                calculate_percentage(field.zstd_size as f64, parent_stats.zstd_size as f64),
                calculate_percentage(
                    field.original_size as f64,
                    parent_stats.original_size as f64
                )
            );
            println!(
                "{:padding$}{} bit, {} unique values, {:?}",
                "",
                field.lenbits,
                field.value_counts.len(),
                field.bit_order
            );
        }
    }

    fn print_concise(&self, schema: &Schema, file_metrics: &FieldMetrics, skip_misc_stats: bool) {
        println!("Schema: {}", self.schema_metadata.name);
        println!(
            "File: {:.2}bpb, {} LZ, {}/{} ({:.2}%/{:.2}%) (zstd/orig)",
            self.file_entropy,
            self.file_lz_matches,
            self.zstd_file_size,
            self.original_size,
            calculate_percentage(self.zstd_file_size as f64, self.original_size as f64),
            100.0
        );

        println!("\nField Metrics:");
        for field_path in schema.ordered_field_and_group_paths() {
            self.concise_print_field(file_metrics, &field_path);
        }

        println!("\nSplit Group Comparisons:");
        for comparison in &self.split_comparisons {
            concise_print_split_comparison(comparison);
        }

        println!("\nCustom Group Comparisons:");
        for comparison in &self.custom_comparisons {
            concise_print_custom_comparison(comparison);
        }

        if !skip_misc_stats {
            println!("\nField Value Stats: [as `value: probability %`]");
            for field_path in schema.ordered_field_and_group_paths() {
                self.concise_print_field_value_stats(&field_path);
            }

            println!("\nField Bit Stats: [as `(zeros/ones) (percentage %)`]");
            for field_path in schema.ordered_field_and_group_paths() {
                self.concise_print_field_bit_stats(&field_path);
            }
        }
    }

    fn concise_print_field(&self, file_metrics: &FieldMetrics, field_path: &str) {
        if let Some(field) = self.per_field.get(field_path) {
            let indent = "  ".repeat(field.depth);
            let parent_stats = field.parent_metrics_or(self, file_metrics);

            println!(
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
            );
        }
    }

    fn concise_print_field_value_stats(&self, field_path: &str) {
        if let Some(field) = self.per_field.get(field_path) {
            print_field_metrics_value_stats(field);
        }
    }

    fn concise_print_field_bit_stats(&self, field_path: &str) {
        if let Some(field) = self.per_field.get(field_path) {
            print_field_metrics_bit_stats(field);
        }
    }
}

fn detailed_print_comparison(comparison: &SplitComparisonResult) {
    concise_print_split_comparison(comparison);
}

fn concise_print_custom_comparison(comparison: &GroupComparisonResult) {
    let base_lz = comparison.baseline_metrics.lz_matches;
    let base_entropy = comparison.baseline_metrics.entropy;
    let base_zstd = comparison.baseline_metrics.zstd_size;
    let base_estimated = comparison.baseline_metrics.estimated_size;
    let base_size = comparison.baseline_metrics.original_size;

    println!("  {}: {}", comparison.name, comparison.description);
    println!("    Base Group:");
    println!("      Size: {}", base_size);
    println!("      LZ, Entropy: ({}, {:.2})", base_lz, base_entropy);
    if base_estimated != 0 {
        println!("      Estimate/Zstd: {}/{}", base_estimated, base_zstd);
    } else {
        println!("      Zstd: {}", base_zstd);
    }

    for (i, (group_name, metrics)) in comparison
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
        let diff_zstd = comparison.differences[i].zstd_size;

        println!("\n    {} Group:", group_name);
        println!("      Size: {}", comp_size);
        println!("      LZ, Entropy: ({}, {:.2})", comp_lz, comp_entropy);
        if comp_estimated != 0 {
            println!("      Estimate/Zstd: {}/{}", comp_zstd, comp_estimated);
        } else {
            println!("      Zstd: {}", comp_zstd);
        }
        println!("      Ratio zstd: {:.1}%", ratio_zstd);
        println!("      Diff zstd: {}", diff_zstd);

        if base_size != comp_size {
            println!("      [WARNING!!] Sizes of base and comparison groups don't match!! They may vary by a few bytes due to padding.");
            println!("      [WARNING!!] However if they vary extremely, your groups may be incorrect. base: {}, {}: {}", base_size, group_name, comp_size);
        }
    }
}

fn concise_print_split_comparison(comparison: &SplitComparisonResult) {
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

    println!("  {}: {}", comparison.name, comparison.description);
    println!("    Original Size: {}", size_orig);
    println!("    Base LZ, Entropy: ({}, {:.2}):", base_lz, base_entropy);
    println!("    Comp LZ, Entropy: ({}, {:.2}):", comp_lz, comp_entropy);
    println!(
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
    );
    println!(
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
    );

    if base_estimated != 0 {
        println!("    Base (est/zstd): {}/{}", base_estimated, base_zstd);
    } else {
        println!("    Base (zstd): {}", base_zstd);
    }

    if comp_estimated != 0 {
        println!("    Comp (est/zstd): {}/{}", comp_estimated, comp_zstd);
    } else {
        println!("    Comp (zstd): {}", comp_zstd);
    }

    println!("    Ratio (zstd): {}", ratio_zstd);
    println!("    Diff (zstd): {}", diff_zstd);

    if size_orig != size_comp {
        println!("    [WARNING!!] Sizes of both groups in bytes don't match!! They may vary by a few bytes due to padding.");
        println!("    [WARNING!!] However if they vary extremely, your groups may be incorrect. group1: {}, group2: {}", size_orig, size_comp);
    }
}
