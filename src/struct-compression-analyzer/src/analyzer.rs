use super::schema::{Group, Schema};
use crate::schema::{BitOrder, FieldDefinition};
use bitstream_io::{BigEndian, BitRead, BitReader, BitWrite, BitWriter, LittleEndian};
use std::{collections::HashMap, io::Cursor};

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
struct FieldStats {
    /// Name of the field or group
    name: String,
    /// Name of the full path to the field or group
    full_path: String,
    /// The depth of the field in the group/field chain.
    depth: usize,
    /// Total number of observed values
    count: u64,
    /// Length of the field or group in bits.
    lenbits: u32,
    /// Bitstream writer for accumulating data in the correct bit order
    writer: BitWriterContainer,
    /// Bit-level statistics. Index of tuple is bit offset.
    bit_counts: Vec<BitStats>,
    /// The order of the bits within the field
    bit_order: BitOrder,
    /// Count of occurrences for each observed value
    value_counts: HashMap<u64, u64>,
}

/// Tracks statistics about individual bits in a field
///
/// Maintains counts of zero and one values observed at each bit position
/// to support entropy calculations and bit distribution analysis.
enum BitWriterContainer {
    Msb(BitWriter<Cursor<Vec<u8>>, BigEndian>),
    Lsb(BitWriter<Cursor<Vec<u8>>, LittleEndian>),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct BitStats {
    /// Count of zero values observed at this bit position
    pub zeros: u64,
    /// Count of one values observed at this bit position
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
        self.entries.extend_from_slice(entry);
        let mut reader = BitReader::endian(entry, BigEndian);
        self.process_group(&self.schema.root, &mut reader);
    }

    fn process_group(&mut self, group: &Group, reader: &mut BitReader<&[u8], BigEndian>) {
        for (name, field_def) in &group.fields {
            match field_def {
                FieldDefinition::Field(field) => {
                    let bits_left = field.bits;
                    let field_stats = self
                        .field_stats
                        .iter_mut()
                        .find(|s| s.name == *name)
                        .unwrap(); // exists by definition
                    process_field_or_group(reader, bits_left, field_stats);
                }
                FieldDefinition::Group(child_group) => {
                    let bits_left = child_group.bits;
                    let field_stats = self
                        .field_stats
                        .iter_mut()
                        .find(|s| s.name == *name)
                        .unwrap(); // exists by definition
                    process_field_or_group(reader, bits_left, field_stats);

                    // Process nested fields
                    self.process_group(child_group, reader);
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

fn process_field_or_group(
    reader: &mut BitReader<&[u8], BigEndian>,
    mut bits_left: u32,
    field_stats: &mut FieldStats,
) {
    let writer = &mut field_stats.writer;

    // Update statistics
    field_stats.count += 1;
    while bits_left > 0 {
        // Read max possible number of bits at once.
        let max_bits = bits_left.min(64);
        let bits = reader.read::<u64>(max_bits).unwrap();

        // Update the value counts
        *field_stats.value_counts.entry(bits).or_insert(0) += 1;

        // Write the values to the output
        match writer {
            BitWriterContainer::Msb(writer) => {
                writer.write(max_bits, bits).unwrap();
            }
            BitWriterContainer::Lsb(writer) => {
                writer.write(max_bits, bits).unwrap();
            }
        }

        // Update stats for individual bits.
        for i in 0..max_bits {
            let bit_value = (bits >> (max_bits - 1 - i)) & 1;
            if bit_value == 0 {
                field_stats.bit_counts[i as usize].zeros += 1;
            } else {
                field_stats.bit_counts[i as usize].ones += 1;
            }
        }

        bits_left -= max_bits;
    }

    // Flush any remaining bits to ensure all data is written
    match writer {
        BitWriterContainer::Msb(writer) => writer.flush().unwrap(),
        BitWriterContainer::Lsb(writer) => writer.flush().unwrap(),
    }
}

/// Recursively builds field statistics structures from schema definition
/// Creates a BitWriterContainer based on the specified bit order
fn create_bit_writer(bit_order: BitOrder) -> BitWriterContainer {
    match bit_order.get_with_default_resolve() {
        BitOrder::Msb => {
            BitWriterContainer::Msb(BitWriter::endian(Cursor::new(Vec::new()), BigEndian))
        }
        BitOrder::Lsb => {
            BitWriterContainer::Lsb(BitWriter::endian(Cursor::new(Vec::new()), LittleEndian))
        }
        _ => unreachable!(),
    }
}

fn build_field_stats<'a>(group: &'a Group, parent_path: &'a str, depth: usize) -> Vec<FieldStats> {
    let mut stats = Vec::new();

    for (name, field) in &group.fields {
        let path = if parent_path.is_empty() {
            name.clone()
        } else {
            format!("{}.{}", parent_path, name)
        };

        match field {
            FieldDefinition::Field(field) => {
                let writer = create_bit_writer(field.bit_order);

                stats.push(FieldStats {
                    full_path: path,
                    depth,
                    lenbits: field.bits,
                    count: 0,
                    writer,
                    bit_counts: vec![BitStats::default(); field.bits as usize],
                    name: name.clone(),
                    bit_order: field.bit_order.get_with_default_resolve(),
                    value_counts: HashMap::new(),
                });
            }
            FieldDefinition::Group(group) => {
                let writer = create_bit_writer(group.bit_order);

                // Add stats entry for the group itself
                stats.push(FieldStats {
                    full_path: path.clone(),
                    depth,
                    lenbits: group.bits,
                    count: 0,
                    writer,
                    bit_counts: vec![BitStats::default(); group.bits as usize],
                    name: name.clone(),
                    bit_order: group.bit_order.get_with_default_resolve(),
                    value_counts: HashMap::new(),
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

#[cfg(test)]
mod tests {

    use super::*;
    use crate::schema::Schema;

    fn create_test_schema() -> Schema {
        let yaml = r###"
version: '1.0'
root:
  type: group
  fields:
    id:
      type: field
      bits: 32
      description: "ID field"
    nested:
      type: group
      bit_order: lsb
      fields:
        value:
          type: field
          bits: 8
          description: "Nested value"
        "###;

        Schema::from_yaml(yaml).expect("Failed to parse test schema")
    }

    #[test]
    fn test_analyzer_initialization() {
        let schema = create_test_schema();
        let analyzer = SchemaAnalyzer::new(&schema);

        // Should collect stats for all fields and groups
        assert_eq!(
            analyzer.field_stats.len(),
            3,
            "Should have stats for root group + 2 fields"
        );
    }

    #[test]
    fn test_big_endian_bitorder() {
        let yaml = r###"
version: '1.0'
root:
  type: group
  fields:
    flags:
      type: field
      bits: 2
      bit_order: msb
"###;
        let schema = Schema::from_yaml(yaml).expect("Failed to parse test schema");
        let mut analyzer = SchemaAnalyzer::new(&schema);

        // Add 4 entries (2 bits each) to make exactly 1 byte (8 bits)
        // Values: 0b11, 0b00, 0b10, 0b01 → combined as 0b11001001 (0xC9)
        analyzer.add_entry(&[0b11000000]); // 0b11 in first 2 bits
        analyzer.add_entry(&[0b00000000]); // 0b00
        analyzer.add_entry(&[0b10000000]); // 0b10
        analyzer.add_entry(&[0b01000000]); // 0b01

        let flags_field = analyzer
            .field_stats
            .iter_mut()
            .find(|s| s.name == "flags")
            .unwrap();

        assert_eq!(flags_field.count, 4, "Should process 4 entries");
        assert_eq!(
            flags_field.bit_counts.len(),
            2,
            "Should track 2 bits per field"
        );

        // Check writer accumulated correct bits
        match &mut flags_field.writer {
            BitWriterContainer::Msb(writer) => {
                // Get a reference to the writer's underlying buffer without moving ownership
                writer.flush().unwrap();
                let inner_writer = writer.writer().unwrap();
                let data = inner_writer.get_ref();
                assert_eq!(*data, vec![0xC9_u8], "Combined bits should form 0xC9");

                // Check value counts
                let expected_counts = HashMap::from([(0b11, 1), (0b00, 1), (0b10, 1), (0b01, 1)]);
                assert_eq!(
                    flags_field.value_counts, expected_counts,
                    "Value counts should match"
                );
            }
            _ => panic!("Expected MSB bit writer"),
        }

        // Check bit counts (each bit position should have 2 zeros and 2 ones)
        for (x, stats) in flags_field.bit_counts.iter().enumerate() {
            assert_eq!(
                stats.zeros, 2,
                "Bit {} should have 2 zeros (actual: {})",
                x, stats.zeros
            );
            assert_eq!(
                stats.ones, 2,
                "Bit {} should have 2 ones (actual: {})",
                x, stats.ones
            );
        }
    }

    #[test]
    fn test_field_stats_structure() {
        let schema = create_test_schema();
        let analyzer = SchemaAnalyzer::new(&schema);

        // Verify field hierarchy and properties
        let root_group = &analyzer.field_stats[0];
        assert_eq!(root_group.name, "id");
        assert_eq!(root_group.full_path, "id");
        assert_eq!(root_group.depth, 0);
        assert_eq!(root_group.count, 0);
        assert_eq!(root_group.lenbits, 32);
        assert!(matches!(root_group.writer, BitWriterContainer::Msb(_)));
        assert_eq!(root_group.bit_counts.len(), root_group.lenbits as usize);
        assert_eq!(root_group.bit_order, BitOrder::Msb);

        let id_field = &analyzer.field_stats[1];
        assert_eq!(id_field.full_path, "nested");
        assert_eq!(id_field.name, "nested");
        assert_eq!(id_field.depth, 0);
        assert_eq!(id_field.count, 0);
        assert_eq!(id_field.lenbits, 8);
        assert!(matches!(id_field.writer, BitWriterContainer::Lsb(_)));
        assert_eq!(id_field.bit_counts.len(), id_field.lenbits as usize);
        assert_eq!(id_field.bit_order, BitOrder::Lsb);

        let nested_value = &analyzer.field_stats[2];
        assert_eq!(nested_value.full_path, "nested.value");
        assert_eq!(nested_value.name, "value");
        assert_eq!(nested_value.depth, 1);
        assert_eq!(nested_value.count, 0);
        assert_eq!(nested_value.lenbits, 8);
        assert!(matches!(nested_value.writer, BitWriterContainer::Lsb(_)));
        assert_eq!(nested_value.bit_counts.len(), nested_value.lenbits as usize);
        assert_eq!(nested_value.bit_order, BitOrder::Lsb); // inherited from parent
    }
}
