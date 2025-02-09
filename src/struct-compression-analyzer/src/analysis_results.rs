use crate::analyze_utils::get_zstd_compressed_size;
use crate::analyze_utils::size_estimate;
use crate::analyzer::get_writer_buffer;
use crate::analyzer::BitStats;
use crate::analyzer::SchemaAnalyzer;
use crate::constants::CHILD_MARKER;
use crate::schema::BitOrder;
use crate::schema::Metadata;
use crate::schema::Schema;
use ahash::AHashMap;
use ahash::HashMapExt;
use derive_more::derive::FromStr;
use lossless_transform_utils::entropy::code_length_of_histogram32;
use lossless_transform_utils::histogram::histogram32_from_bytes;
use lossless_transform_utils::histogram::Histogram32;
use lossless_transform_utils::match_estimator::estimate_num_lz_matches_fast;
use rayon::iter::IntoParallelRefMutIterator;
use rayon::iter::ParallelIterator;
use rustc_hash::FxHashMap;

pub fn compute_analysis_results(analyzer: &mut SchemaAnalyzer) -> AnalysisResults {
    // First calculate file entropy
    let file_entropy = calculate_file_entropy(&analyzer.entries);
    let file_lz_matches = estimate_num_lz_matches_fast(&analyzer.entries);

    // Then calculate per-field entropy and lz matches
    let mut field_metrics: AHashMap<String, FieldMetrics> = AHashMap::new();

    for stats in &mut analyzer.field_stats.values_mut() {
        let writer_buffer = get_writer_buffer(&mut stats.writer);
        let entropy = calculate_file_entropy(writer_buffer);
        let lz_matches = estimate_num_lz_matches_fast(writer_buffer);
        let estimated_size = size_estimate(writer_buffer, lz_matches, entropy);
        let actual_size = get_zstd_compressed_size(writer_buffer);

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
                estimated_size,
                zstd_size: actual_size,
                original_size: writer_buffer.len(),
            },
        );
    }

    // Process group comparisons
    let mut group_comparisons = Vec::new();
    for comparison in &analyzer.schema.analysis.compare_groups {
        let mut group1_bytes: Vec<u8> = Vec::new();
        let mut group2_bytes: Vec<u8> = Vec::new();

        // Sum up bytes for group 1
        for name in &comparison.group_1 {
            if let Some(stats) = analyzer.field_stats.get_mut(name) {
                group1_bytes.extend_from_slice(get_writer_buffer(&mut stats.writer));
            }
        }

        // Sum up bytes for group 2
        for name in &comparison.group_2 {
            if let Some(stats) = analyzer.field_stats.get_mut(name) {
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

        group_comparisons.push(GroupComparisonResult {
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

    AnalysisResults {
        file_entropy,
        file_lz_matches,
        per_field: field_metrics,
        schema_metadata: analyzer.schema.metadata.clone(),
        estimated_file_size: size_estimate(&analyzer.entries, file_lz_matches, file_entropy),
        zstd_file_size: get_zstd_compressed_size(&analyzer.entries),
        original_size: analyzer.entries.len(),
        group_comparisons,
    }
}

fn calculate_file_entropy(bytes: &[u8]) -> f64 {
    let mut histogram = Histogram32::default();
    histogram32_from_bytes(bytes, &mut histogram);
    code_length_of_histogram32(&histogram, bytes.len() as u64)
}

/// Final computed metrics for output
#[derive(Clone)]
pub struct AnalysisResults {
    /// Schema name
    pub schema_metadata: Metadata,

    /// Entropy of the whole file
    pub file_entropy: f64,

    /// LZ compression matches in the file
    pub file_lz_matches: usize,

    /// Estimated size of the compressed data from our estimator
    pub estimated_file_size: usize,

    /// Actual size of the compressed data when compressed with zstandard
    pub zstd_file_size: usize,

    /// Original size of the uncompressed data
    pub original_size: usize,

    /// Field path → computed metrics
    /// This is a map of `full_path` to [`FieldMetrics`], such that we
    /// can easily merge the results of different fields down the road.
    pub per_field: AHashMap<String, FieldMetrics>,

    /// Group comparison results
    pub group_comparisons: Vec<GroupComparisonResult>,
}

/// Complete analysis metrics for a single field
#[derive(Clone)]
pub struct FieldMetrics {
    /// Name of the field or group
    pub name: String,
    /// Name of the full path to the field or group
    pub full_path: String,
    /// The depth of the field in the group/field chain.
    pub depth: usize,
    /// Total number of observed values
    pub count: u64,
    /// Length of the field or group in bits.
    pub lenbits: u32,
    /// Shannon entropy in bits
    pub entropy: f64,
    /// LZ compression matches in the field
    pub lz_matches: usize,
    /// Bit-level statistics. Index of tuple is bit offset.
    pub bit_counts: Vec<BitStats>,
    /// The order of the bits within the field
    pub bit_order: BitOrder,
    /// Value → occurrence count
    /// Count of occurrences for each observed value.
    pub value_counts: FxHashMap<u64, u64>,
    /// Estimated size of the compressed data from our estimator
    pub estimated_size: usize,
    /// Actual size of the compressed data when compressed with zstandard
    pub zstd_size: usize,
    /// Original size of the data before compression
    pub original_size: usize,
}

impl FieldMetrics {
    /// Merge two [`FieldMetrics`] objects into one.
    /// This is useful when analyzing multiple files or groups of fields.
    ///
    /// # Arguments
    ///
    /// * `self` - The object to merge into.
    /// * `other` - The object to merge from.
    fn merge_many(&mut self, other: &[&FieldMetrics]) {
        // Add counts from others
        self.count += other.iter().map(|m| m.count).sum::<u64>();

        // Merge count, entropy
        let entropy = (self.entropy + other.iter().map(|m| m.entropy).sum::<f64>())
            / (other.len() + 1) as f64;
        let lz_matches = (self.lz_matches + other.iter().map(|m| m.lz_matches).sum::<usize>())
            / (other.len() + 1);
        self.entropy = entropy;
        self.lz_matches = lz_matches;

        // Merge estimated and actual sizes
        let estimated_size = (self.estimated_size
            + other.iter().map(|m| m.estimated_size).sum::<usize>())
            / (other.len() + 1);
        let actual_size =
            (self.zstd_size + other.iter().map(|m| m.zstd_size).sum::<usize>()) / (other.len() + 1);
        self.estimated_size = estimated_size;
        self.zstd_size = actual_size;
        self.original_size = (self.original_size
            + other.iter().map(|m| m.original_size).sum::<usize>())
            / (other.len() + 1);

        // Sum up arrays from both items
        let bit_counts = &mut self.bit_counts;
        for stats in other.iter() {
            // Add bit counts from others into self
            for (bit_offset, bit_stats) in stats.bit_counts.iter().enumerate() {
                let current_counts = bit_counts.get_mut(bit_offset).unwrap();
                current_counts.ones += bit_stats.ones;
                current_counts.zeros += bit_stats.zeros;
            }

            // Add value counts from others into self
            for (value, count) in stats.value_counts.iter() {
                if let Some(existing_count) = self.value_counts.get_mut(value) {
                    *existing_count += count;
                } else {
                    self.value_counts.insert(*value, *count);
                }
            }
        }
    }

    /// Returns the parent path of the current field.
    /// The parent path is the part of the full path before the last dot.
    pub fn parent_path(&self) -> Option<&str> {
        get_parent_path(&self.full_path)
    }

    /// Returns the [`FieldMetrics`] object for the parent of the current field.
    /// Returns `None` if there is no parent.
    pub fn parent_metrics_or<'a>(
        &self,
        results: &'a AnalysisResults,
        optb: &'a FieldMetrics,
    ) -> &'a FieldMetrics {
        let parent_path = self.parent_path();
        let parent_stats = parent_path
            .and_then(|p| results.per_field.get(p))
            .unwrap_or(optb);
        parent_stats
    }

    /// Get sorted value counts descending (value, count)
    pub fn sorted_value_counts(&self) -> Vec<(&u64, &u64)> {
        let mut counts: Vec<_> = self.value_counts.iter().collect();
        counts.sort_by(|a, b| b.1.cmp(a.1));
        counts
    }
}

pub(crate) fn get_parent_path(field_path: &str) -> Option<&str> {
    field_path.rsplit_once(CHILD_MARKER).map(|(p, _)| p)
}

#[derive(Debug, Clone, Copy, Default, FromStr)]
pub enum PrintFormat {
    #[default]
    Detailed,
    Concise,
}

impl AnalysisResults {
    /// Merge multiple [`AnalysisResults`] objects into one.
    /// This is useful when analyzing multiple files or groups of fields.
    pub fn merge_many(&mut self, other: &[AnalysisResults]) {
        // For each field in current item, find equivalent field in multiple others, and merge them
        self.per_field
            .par_iter_mut()
            .for_each(|(full_path, field_metrics)| {
                // Get all matching `full_path` from all other elements as vec
                let matches: Vec<&FieldMetrics> = other
                    .iter()
                    .flat_map(|results| results.per_field.get(full_path))
                    .collect();

                // Now merge as one operation
                field_metrics.merge_many(&matches);
            });

        // Merge the file entropy and LZ matches
        self.file_entropy = (self.file_entropy + other.iter().map(|m| m.file_entropy).sum::<f64>())
            / (other.len() + 1) as f64;
        self.file_lz_matches = (self.file_lz_matches
            + other.iter().map(|m| m.file_lz_matches).sum::<usize>())
            / (other.len() + 1);
    }

    /// Converts the file level statistics into a [`FieldMetrics`] object
    /// which can be used for comparison with parent in places such as the
    /// print function.
    pub fn as_field_metrics(&self) -> FieldMetrics {
        FieldMetrics {
            name: String::new(),
            full_path: String::new(),
            depth: 0,
            zstd_size: self.zstd_file_size,
            estimated_size: self.estimated_file_size,
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
        println!("File Estimated Size: {}", self.estimated_file_size);
        println!("\nPer-field Metrics (in schema order):");

        // Iterate through schema-defined fields in order
        for field_path in schema.ordered_field_paths() {
            self.detailed_print_field(file_metrics, &field_path);
        }

        println!("\nGroup Comparisons:");
        for comparison in &self.group_comparisons {
            detailed_print_comparison(comparison);
        }

        if !skip_misc_stats {
            println!("\nField Value Stats: [as `value: probability %`]");
            for field_path in schema.ordered_field_paths() {
                self.concise_print_field_value_stats(&field_path);
            }

            println!("\nField Bit Stats: [as `(zeros/ones) (percentage %)`]");
            for field_path in schema.ordered_field_paths() {
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
                "{:padding$}Sizes: Estimated/ZStandard -16/Original: {}/{}/{} ({:.2}%/{:.2}%/{:.2}%)",
                "",
                field.estimated_size,
                field.zstd_size,
                field.original_size,
                calculate_percentage(
                    field.estimated_size as f64,
                    parent_stats.estimated_size as f64
                ),
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
            "File: {:.2}bpb, {} LZ, {}/{}/{} ({:.2}%/{:.2}%/{:.2}%) (est/zstd/orig)",
            self.file_entropy,
            self.file_lz_matches,
            self.estimated_file_size,
            self.zstd_file_size,
            self.original_size,
            calculate_percentage(self.estimated_file_size as f64, self.original_size as f64),
            calculate_percentage(self.zstd_file_size as f64, self.original_size as f64),
            100.0
        );

        println!("\nField Metrics:");
        for field_path in schema.ordered_field_paths() {
            self.concise_print_field(file_metrics, &field_path);
        }

        println!("\nGroup Comparisons:");
        for comparison in &self.group_comparisons {
            concise_print_comparison(comparison);
        }

        if !skip_misc_stats {
            println!("\nField Value Stats: [as `value: probability %`]");
            for field_path in schema.ordered_field_paths() {
                self.concise_print_field_value_stats(&field_path);
            }

            println!("\nField Bit Stats: [as `(zeros/ones) (percentage %)`]");
            for field_path in schema.ordered_field_paths() {
                self.concise_print_field_bit_stats(&field_path);
            }
        }
    }

    fn concise_print_field(&self, file_metrics: &FieldMetrics, field_path: &str) {
        if let Some(field) = self.per_field.get(field_path) {
            let indent = "  ".repeat(field.depth);
            let parent_stats = field.parent_metrics_or(self, file_metrics);

            println!(
                "{}{}: {:.2}bpb, {} LZ ({:.2}%), {}/{}/{} ({:.2}%/{:.2}%/{:.2}%) (est/zstd/orig), {}bit",
                indent,
                field.name,
                field.entropy,
                field.lz_matches,
                calculate_percentage(field.lz_matches as f64, parent_stats.lz_matches as f64),
                field.estimated_size,
                field.zstd_size,
                field.original_size,
                calculate_percentage(field.estimated_size as f64, parent_stats.estimated_size as f64),
                calculate_percentage(field.zstd_size as f64, parent_stats.zstd_size as f64),
                calculate_percentage(field.original_size as f64, parent_stats.original_size as f64),
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
    /// The size difference between the two groups.
    pub group1_field_metrics: Vec<FieldMetrics>,
    /// The size difference between the two groups.
    pub group2_field_metrics: Vec<FieldMetrics>,
}

impl GroupComparisonResult {
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

// Helper function to calculate percentage
fn calculate_percentage(child: f64, parent: f64) -> f64 {
    if parent == 0.0 {
        0.0
    } else {
        (child / parent) * 100.0
    }
}

fn detailed_print_comparison(comparison: &GroupComparisonResult) {
    concise_print_comparison(comparison);
}

fn print_field_metrics_value_stats(field: &FieldMetrics) {
    // Print field name with indent
    let indent = "  ".repeat(field.depth);
    println!("{}{} ({} bits)", indent, field.name, field.lenbits);

    // Print value statistics
    let counts = field.sorted_value_counts();
    if !counts.is_empty() {
        let total_values: u64 = counts.iter().map(|(_, &c)| c).sum();
        for (val, &count) in counts.iter().take(5) {
            let pct = (count as f32 / total_values as f32) * 100.0;
            println!("{}    {}: {:.1}%", indent, val, pct);
        }
    }
}

fn print_field_metrics_bit_stats(field: &FieldMetrics) {
    let indent = "  ".repeat(field.depth);
    println!("{}{} ({} bits)", indent, field.name, field.lenbits);

    for i in 0..field.lenbits {
        let bit_stats = &field.bit_counts[i as usize];
        let total = bit_stats.zeros + bit_stats.ones;
        let percentage = if total > 0 {
            (bit_stats.ones as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        println!(
            "{}  Bit {}: ({}/{}) ({:.1}%)",
            indent, i, bit_stats.zeros, bit_stats.ones, percentage
        );
    }
}

fn concise_print_comparison(comparison: &GroupComparisonResult) {
    let base_lz = comparison.group1_metrics.lz_matches;
    let size_orig = comparison.group1_metrics.original_size;
    let size_comp = comparison.group2_metrics.original_size;
    let base_entropy = comparison.group1_metrics.entropy;

    let base_est = comparison.group1_metrics.estimated_size;
    let base_zstd = comparison.group1_metrics.zstd_size;

    let comp_lz = comparison.group2_metrics.lz_matches;
    let comp_entropy = comparison.group2_metrics.entropy;

    let comp_est = comparison.group2_metrics.estimated_size;
    let comp_zstd = comparison.group2_metrics.zstd_size;

    let ratio_est = calculate_percentage(comp_est as f64, base_est as f64);
    let ratio_zstd = calculate_percentage(comp_zstd as f64, base_zstd as f64);

    let diff_est = comparison.difference.estimated_size;
    let diff_zstd = comparison.difference.zstd_size;

    println!("  {}: {}", comparison.name, comparison.description);
    println!("    Original Size: {}", size_orig);
    println!("    Base LZ, Entropy: ({}, {:.2}):", base_lz, base_entropy);
    println!("    Comp LZ, Entropy: ({}, {:.2}):", comp_lz, comp_entropy);
    println!(
        "    Base Group LZ, Entropy: ({:?}, {:?})",
        comparison
            .group1_field_metrics
            .iter()
            .map(|m| m.lz_matches)
            .collect::<Vec<_>>(),
        comparison
            .group1_field_metrics
            .iter()
            .map(|m| format!("{:.2}", m.entropy))
            .collect::<Vec<_>>()
    );
    println!(
        "    Comp Group LZ, Entropy: ({:?}, {:?})",
        comparison
            .group2_field_metrics
            .iter()
            .map(|m| m.lz_matches)
            .collect::<Vec<_>>(),
        comparison
            .group2_field_metrics
            .iter()
            .map(|m| format!("{:.2}", m.entropy))
            .collect::<Vec<_>>()
    );

    println!("    Base (est/zstd): ({}/{})", base_est, base_zstd);
    println!("    Comp (est/zstd): ({}/{})", comp_est, comp_zstd);
    println!("    Ratio (est/zstd): ({}/{})", ratio_est, ratio_zstd);
    println!("    Diff (est/zstd): ({}/{})", diff_est, diff_zstd);

    if size_orig != size_comp {
        println!("    [WARNING!!] Sizes of both groups in bytes don't match!! They may vary by a few bytes due to padding.");
        println!("    [WARNING!!] However if they vary extremely, your groups may be incorrect. group1: {}, group2: {}", size_orig, size_comp);
    }
}
