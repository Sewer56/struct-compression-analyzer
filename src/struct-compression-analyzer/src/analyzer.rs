use crate::schema::FieldDefinition;

use super::schema::{Group, Schema};
use std::collections::HashMap;

/// Analyzes binary structures against a schema definition
///
/// Maintains state between data ingestion and final analysis:
/// - Parsed schema structure
/// - Accumulated raw data entries
/// - Intermediate analysis state
pub struct SchemaAnalyzer<'a> {
    /// Schema definition tree
    schema: &'a Schema,
    /// Raw data as fed into the analyzer.
    entries: Vec<u8>,
    /// Intermediate analysis state (field name → statistics)
    /// This supports both 'groups' and fields.
    field_stats: Vec<FieldStats>,
}

/// Intermediate statistics for a single field or group of fields
#[derive(Default)]
struct FieldStats {
    /// Name of the field or group
    name: String,
    /// The depth of the field in the group/field chain.
    depth: usize,
    /// Total number of observed values
    count: u64,
    /// Length of the field or group in bits.
    lenbits: u32,
    /// All of the data that fits under this field/group of fields. For entropy / match / frequency calculations.
    data: Vec<u8>,
    /// Bit-level statistics. Index of tuple is bit offset.
    /// Value is
    bit_counts: Vec<BitStats>,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
struct BitStats {
    pub zeros: u64,
    pub ones: u64,
}

impl<'a> SchemaAnalyzer<'a> {
    /// Creates a new analyzer bound to a specific schema
    ///
    /// # Example
    /// ```rust
    /// # use struct_compression_analyzer::{schema::Schema, analyzer::SchemaAnalyzer};
    /// # let schema = Schema::from_yaml("version: '1.0'\nroot: { type: group, fields: {} }").unwrap();
    /// let analyzer = SchemaAnalyzer::new(&schema);
    /// ```
    pub fn new(schema: &'a Schema) -> Self {
        Self {
            schema,
            entries: Vec::new(),
            field_stats: build_field_stats(&schema.root, "", 0),
        }
    }

    /// Ingests a raw binary entry for analysis
    ///
    /// # Arguments
    /// * `entry` - Byte slice representing one instance of the schema structure
    ///
    /// # Notes
    /// - Byte order is assumed to be big-endian
    /// - Partial entries will be handled in future implementations
    pub fn add_entry(&mut self, entry: &[u8]) {
        // Extend the total data state.
        self.entries.extend_from_slice(entry);
        for field in self.schema.root.fields.values() {}
    }

    fn process_field(&mut self, parent_path: &str, field: &FieldDefinition) {
        match field {
            FieldDefinition::Field(field) => {
                // Field processing logic will be added here
            }
            FieldDefinition::Group(group) => {
                for (name, nested_field) in &group.fields {
                    let path = format!("{}.{}", parent_path, name);
                    self.process_field(&path, nested_field);
                }
            }
        }
    }

    /// Generates final analysis results
    ///
    /// # Returns
    /// Computed metrics including:
    /// - Entropy calculations
    /// - Bit distribution statistics
    /// - Value frequency analysis
    pub fn generate_results(&self) -> AnalysisResults {
        AnalysisResults {
            // TODO: Convert field_stats to computed metrics
            per_field: HashMap::new(),
        }
    }
}

/// Recursively builds field statistics structures from schema definition
fn build_field_stats(group: &Group, parent_path: &str, depth: usize) -> Vec<FieldStats> {
    let mut stats = Vec::new();

    for (name, field) in &group.fields {
        let path = if parent_path.is_empty() {
            name.clone()
        } else {
            format!("{}.{}", parent_path, name)
        };

        match field {
            FieldDefinition::Field(field) => {
                stats.push(FieldStats {
                    name: path,
                    depth,
                    lenbits: field.bits,
                    count: 0,
                    data: Vec::new(),
                    bit_counts: vec![BitStats::default(); field.bits as usize],
                });
            }
            FieldDefinition::Group(group) => {
                // Add stats entry for the group itself
                stats.push(FieldStats {
                    name: path.clone(),
                    depth,
                    lenbits: group.bits,
                    count: 0,
                    data: Vec::new(),
                    bit_counts: Vec::new(),
                });

                // Process nested fields
                stats.extend(build_field_stats(group, &path, depth + 1));
            }
        }
    }

    stats
}

/// Final computed metrics for output
pub struct AnalysisResults {
    /// Field path → computed metrics
    per_field: HashMap<String, FieldMetrics>,
}

/// Complete analysis metrics for a single field
pub struct FieldMetrics {
    /// Shannon entropy in bits
    pub entropy: f64,
    /// Bit position → (zero_probability, one_probability)
    pub bit_distribution: Vec<(f64, f64)>,
    /// Value → occurrence count
    pub value_counts: HashMap<u64, u64>,
}
