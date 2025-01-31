use crate::analyzer::get_writer_buffer;
use crate::analyzer::BitStats;
use crate::analyzer::SchemaAnalyzer;
use crate::schema::BitOrder;
use crate::schema::Metadata;
use lossless_transform_utils::entropy::code_length_of_histogram32;
use lossless_transform_utils::histogram::histogram32_from_bytes;
use lossless_transform_utils::histogram::Histogram32;
use lossless_transform_utils::match_estimator::estimate_num_lz_matches_fast;
use std::collections::HashMap;

pub fn compute_analysis_results(analyzer: &mut SchemaAnalyzer) -> AnalysisResults {
    // First calculate file entropy
    let file_entropy = calculate_file_entropy(&analyzer.entries);
    let file_lz_matches = estimate_num_lz_matches_fast(&analyzer.entries);

    // Then calculate per-field entropy and lz matches
    let mut field_metrics: HashMap<String, FieldMetrics> = HashMap::new();

    for stats in &mut analyzer.field_stats {
        let writer_buffer = get_writer_buffer(&mut stats.writer);
        let entropy = calculate_file_entropy(writer_buffer);
        let lz_matches = estimate_num_lz_matches_fast(writer_buffer);

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
            },
        );
    }

    AnalysisResults {
        file_entropy,
        file_lz_matches,
        per_field: field_metrics,
        schema_metadata: analyzer.schema.metadata.clone(),
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

    /// Field path → computed metrics
    /// This is a map of `full_path` to [`FieldMetrics`], such that we
    /// can easily merge the results of different fields down the road.
    pub per_field: HashMap<String, FieldMetrics>,
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
    pub value_counts: HashMap<u64, u64>,
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
}

impl AnalysisResults {
    /// Merge multiple [`AnalysisResults`] objects into one.
    /// This is useful when analyzing multiple files or groups of fields.
    pub fn merge_many(&mut self, other: &[AnalysisResults]) {
        // For each field in current item, find equivalent field in multiple others, and merge them
        for (full_path, field_metrics) in &mut self.per_field {
            // Get all matching `full_path` from all other elements as vec
            let matches: Vec<&FieldMetrics> = other
                .iter()
                .flat_map(|results| results.per_field.get(full_path))
                .collect();

            // Now merge as one operation
            field_metrics.merge_many(&matches);
        }

        // Merge the file entropy and LZ matches
        self.file_entropy = (self.file_entropy + other.iter().map(|m| m.file_entropy).sum::<f64>())
            / (other.len() + 1) as f64;
        self.file_lz_matches = (self.file_lz_matches
            + other.iter().map(|m| m.file_lz_matches).sum::<usize>())
            / (other.len() + 1);
    }

    pub fn print(&self) {
        println!("Schema: {}", self.schema_metadata.name);
        println!("Description: {}", self.schema_metadata.description);
        println!("File Entropy: {:.2} bits", self.file_entropy);
        println!("File LZ Matches: {}", self.file_lz_matches);
        println!("\nPer-field Metrics:");

        // Collect and sort fields by their full path for consistent ordering
        let mut fields: Vec<&FieldMetrics> = self.per_field.values().collect();
        fields.sort_by(|a, b| a.full_path.cmp(&b.full_path));

        for field in fields {
            // Indent based on field depth to show hierarchy
            let indent = "  ".repeat(field.depth);
            println!(
                "{}{}: {:.2} bits entropy, {} LZ matches, {} values, {} bits, {} unique, {:?} order",
                indent,
                field.name,
                field.entropy,
                field.lz_matches,
                field.count,
                field.lenbits,
                field.value_counts.len(),
                field.bit_order
            );
        }
    }
}
