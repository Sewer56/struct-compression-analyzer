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
pub struct AnalysisResults {
    /// Schema name
    pub schema_metadata: Metadata,

    /// Entropy of the whole file
    pub file_entropy: f64,

    /// LZ compression matches in the file
    pub file_lz_matches: usize,

    /// Field path → computed metrics
    /// This is a map of `full_path` to `FieldMetrics`, such that we
    /// can easily merge the results of different fields down the road.
    pub per_field: HashMap<String, FieldMetrics>,
}

/// Complete analysis metrics for a single field
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
