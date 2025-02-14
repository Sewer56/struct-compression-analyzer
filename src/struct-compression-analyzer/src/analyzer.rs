use super::schema::{Group, Schema};
use crate::analyze_utils::{create_bit_reader, reverse_bits, BitReaderContainer};
use crate::constants::CHILD_MARKER;
use crate::{
    analysis_results::{compute_analysis_results, AnalysisResults},
    schema::{BitOrder, Condition, FieldDefinition},
};
use ahash::{AHashMap, HashMapExt};
use bitstream_io::{BigEndian, BitRead, BitReader, BitWrite, BitWriter, Endianness};
use rustc_hash::FxHashMap;
use std::io::{Cursor, SeekFrom};

/// Analyzes binary structures against a schema definition
///
/// Maintains state between data ingestion and final analysis:
/// - Parsed schema structure
/// - Accumulated raw data entries
/// - Intermediate analysis state
pub struct SchemaAnalyzer<'a> {
    /// Schema definition tree
    pub schema: &'a Schema,
    /// Raw data as fed into the analyzer.
    pub entries: Vec<u8>,
    /// Intermediate analysis state (field name → statistics)
    /// This supports both 'groups' and fields.
    pub field_stats: AHashMap<String, FieldStats>,
}

/// Intermediate statistics for a single field or group of fields
pub struct FieldStats {
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
    /// Bitstream writer for accumulating data in the correct bit order
    pub writer: BitWriter<Cursor<Vec<u8>>, BigEndian>,
    /// Bit-level statistics. Index of tuple is bit offset.
    pub bit_counts: Vec<BitStats>,
    /// The order of the bits within the field
    pub bit_order: BitOrder,
    /// Count of occurrences for each observed value
    pub value_counts: FxHashMap<u64, u64>,
}

pub fn get_writer_buffer(writer: &mut BitWriter<Cursor<Vec<u8>>, BigEndian>) -> &[u8] {
    writer.byte_align().unwrap();
    writer.writer().unwrap().get_ref()
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BitStats {
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
        let reader = create_bit_reader(entry, self.schema.bit_order);
        match reader {
            BitReaderContainer::Msb(mut bit_reader) => {
                self.process_group(&self.schema.root, &mut bit_reader)
            }
            BitReaderContainer::Lsb(mut bit_reader) => {
                self.process_group(&self.schema.root, &mut bit_reader)
            }
        }
    }

    fn process_group<TEndian: Endianness>(
        &mut self,
        group: &Group,
        reader: &mut BitReader<Cursor<&[u8]>, TEndian>,
    ) {
        // Check self, if the group should be skipped.
        // Note; this code is here because the 'root' of schema is a group.
        if should_skip(reader, &group.skip_if_not) {
            return;
        }

        for (name, field_def) in &group.fields {
            match field_def {
                FieldDefinition::Field(field) => {
                    // Check if the child field can be skipped.
                    if should_skip(reader, &field.skip_if_not) {
                        continue;
                    }

                    let bits_left = field.bits;
                    let field_stats = self.field_stats.get_mut(name).unwrap(); // exists by definition
                    process_field_or_group(
                        reader,
                        bits_left,
                        field_stats,
                        field.skip_frequency_analysis,
                    );
                }
                FieldDefinition::Group(child_group) => {
                    let bits_left = child_group.bits;
                    let field_stats = self.field_stats.get_mut(name).unwrap(); // exists by definition

                    // Note (processing field/group)
                    let current_offset = reader.position_in_bits().unwrap();
                    process_field_or_group(
                        reader,
                        bits_left,
                        field_stats,
                        child_group.skip_frequency_analysis,
                    );
                    reader.seek_bits(SeekFrom::Start(current_offset)).unwrap();

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
    pub fn generate_results(&mut self) -> AnalysisResults {
        compute_analysis_results(self)
    }
}

fn process_field_or_group<TEndian: Endianness>(
    reader: &mut BitReader<Cursor<&[u8]>, TEndian>,
    mut bit_count: u32,
    field_stats: &mut FieldStats,
    skip_frequency_analysis: bool,
) {
    let writer = &mut field_stats.writer;
    // We don't support value counting for structs >8 bytes.
    let can_bit_stats = bit_count <= 64;
    let skip_count_values = bit_count > 64 || skip_frequency_analysis;

    // Update statistics
    field_stats.count += 1;
    while bit_count > 0 {
        // Read max possible number of bits at once.
        let max_bits = bit_count.min(64);
        let bits = reader.read::<u64>(max_bits).unwrap();

        // Update the value counts
        if !skip_count_values {
            if field_stats.bit_order == BitOrder::Lsb {
                // Reverse bits for lsb order
                let reversed_bits = reverse_bits(max_bits, bits);
                field_stats.value_counts.insert(
                    reversed_bits,
                    field_stats.value_counts.get(&reversed_bits).unwrap_or(&0) + 1,
                );
            } else {
                *field_stats.value_counts.entry(bits).or_insert(0) += 1;
            }
        }

        // Write the values to the output
        writer.write(max_bits, bits).unwrap();

        // Update stats for individual bits.
        if can_bit_stats {
            for i in 0..max_bits {
                let bit_value = (bits >> (max_bits - 1 - i)) & 1;
                if bit_value == 0 {
                    field_stats.bit_counts[i as usize].zeros += 1;
                } else {
                    field_stats.bit_counts[i as usize].ones += 1;
                }
            }
        }

        bit_count -= max_bits;
    }

    // Flush any remaining bits to ensure all data is written
    writer.flush().unwrap();
}

/// Recursively builds field statistics structures from schema definition
fn create_bit_writer() -> BitWriter<Cursor<Vec<u8>>, BigEndian> {
    BitWriter::endian(Cursor::new(Vec::new()), BigEndian)
}

fn build_field_stats<'a>(
    group: &'a Group,
    parent_path: &'a str,
    depth: usize,
) -> AHashMap<String, FieldStats> {
    let mut stats = AHashMap::new();

    for (name, field) in &group.fields {
        let path = if parent_path.is_empty() {
            name.clone()
        } else {
            format!("{}{CHILD_MARKER}{}", parent_path, name)
        };

        match field {
            FieldDefinition::Field(field) => {
                let writer = create_bit_writer();
                stats.insert(
                    name.clone(),
                    FieldStats {
                        full_path: path,
                        depth,
                        lenbits: field.bits,
                        count: 0,
                        writer,
                        bit_counts: vec![BitStats::default(); clamp_bits(field.bits as usize)],
                        name: name.clone(),
                        bit_order: field.bit_order.get_with_default_resolve(),
                        value_counts: FxHashMap::new(),
                    },
                );
            }
            FieldDefinition::Group(group) => {
                let writer = create_bit_writer();

                // Add stats entry for the group itself
                stats.insert(
                    name.clone(),
                    FieldStats {
                        full_path: path.clone(),
                        depth,
                        lenbits: group.bits,
                        count: 0,
                        writer,
                        bit_counts: vec![BitStats::default(); clamp_bits(group.bits as usize)],
                        name: name.clone(),
                        bit_order: group.bit_order.get_with_default_resolve(),
                        value_counts: FxHashMap::new(),
                    },
                );

                // Process nested fields
                stats.extend(build_field_stats(group, &path, depth + 1));
            }
        }
    }

    stats
}

/// Checks if we should skip processing based on conditions
#[inline]
fn should_skip<TEndian: Endianness>(
    reader: &mut BitReader<Cursor<&[u8]>, TEndian>,
    conditions: &[Condition],
) -> bool {
    // Fast return, since there usually are no conditions.
    if conditions.is_empty() {
        return false;
    }

    let original_pos_bits = reader.position_in_bits().unwrap();
    for condition in conditions {
        let offset = (condition.byte_offset * 8) + condition.bit_offset as u64;
        let target_pos = original_pos_bits + offset;
        reader.seek_bits(SeekFrom::Start(target_pos)).unwrap();
        let mut value = reader.read::<u64>(condition.bits as u32).unwrap();

        // Reverse bits if needed
        if condition.bit_order == BitOrder::Lsb {
            value = reverse_bits(condition.bits as u32, value);
        }

        if value != condition.value {
            reader
                .seek_bits(SeekFrom::Start(original_pos_bits))
                .unwrap();
            return true;
        }
    }

    reader
        .seek_bits(SeekFrom::Start(original_pos_bits))
        .unwrap();
    false
}

fn clamp_bits(bits: usize) -> usize {
    if bits > 64 {
        0
    } else {
        bits
    }
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

        // Assert general writing.
        {
            let flags_field = analyzer.field_stats.get_mut("flags").unwrap();
            assert_eq!(flags_field.count, 4, "Should process 4 entries");
            assert_eq!(
                flags_field.bit_counts.len(),
                2,
                "Should track 2 bits per field"
            );

            // Check writer accumulated correct bits
            flags_field.writer.byte_align().unwrap();
            flags_field.writer.flush().unwrap();
            let inner_writer = flags_field.writer.writer().unwrap();
            let data = inner_writer.get_ref();
            assert_eq!(data[0], 0xC9_u8, "Combined bits should form 0xC9");

            // Check value counts
            let expected_counts =
                FxHashMap::from_iter([(0b11, 1), (0b00, 1), (0b10, 1), (0b01, 1)]);
            assert_eq!(
                flags_field.value_counts, expected_counts,
                "Value counts should match"
            );

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

        // Add another entry, to specifically test big endian
        analyzer.add_entry(&[0b01000000]); // 0b01
        let flags_field = analyzer.field_stats.get_mut("flags").unwrap();
        let expected_counts = FxHashMap::from_iter([(0b11, 1), (0b00, 1), (0b10, 1), (0b01, 2)]);
        assert_eq!(
            flags_field.value_counts, expected_counts,
            "Value counts should match"
        );
    }

    #[test]
    fn test_little_endian_bitorder() {
        let yaml = r###"
version: '1.0'
root:
  type: group
  fields:
    flags:
      type: field
      bits: 2
      bit_order: lsb
"###;
        let schema = Schema::from_yaml(yaml).expect("Failed to parse test schema");
        let mut analyzer = SchemaAnalyzer::new(&schema);

        // Add 4 entries (2 bits each) to make exactly 1 byte (8 bits)
        // This is a repeat of the logic from the big endian test.
        analyzer.add_entry(&[0b11000000]); // 0b11 in first 2 bits
        analyzer.add_entry(&[0b00000000]); // 0b00
        analyzer.add_entry(&[0b10000000]); // 0b01 (due to endian flip)
        analyzer.add_entry(&[0b01000000]); // 0b10 (due to endian flip)

        // Asserts for general writing are in the equivalent big endian test.
        // Add another entry, to specifically test little endian
        analyzer.add_entry(&[0b10000000]); // 0b01 (due to endian flip)
        let flags_field = analyzer.field_stats.get_mut("flags").unwrap();
        let expected_counts = FxHashMap::from_iter([(0b11, 1), (0b00, 1), (0b10, 1), (0b01, 2)]);
        assert_eq!(
            flags_field.value_counts, expected_counts,
            "Value counts should match"
        );
    }

    #[test]
    fn test_field_stats_structure() {
        let schema = create_test_schema();
        let analyzer = SchemaAnalyzer::new(&schema);

        // Verify field hierarchy and properties
        let root_group = analyzer.field_stats.get("id").unwrap();
        assert_eq!(root_group.name, "id");
        assert_eq!(root_group.full_path, "id");
        assert_eq!(root_group.depth, 0);
        assert_eq!(root_group.count, 0);
        assert_eq!(root_group.lenbits, 32);
        assert_eq!(root_group.bit_counts.len(), root_group.lenbits as usize);
        assert_eq!(root_group.bit_order, BitOrder::Msb);

        let id_field = analyzer.field_stats.get("nested").unwrap();
        assert_eq!(id_field.full_path, "nested");
        assert_eq!(id_field.name, "nested");
        assert_eq!(id_field.depth, 0);
        assert_eq!(id_field.count, 0);
        assert_eq!(id_field.lenbits, 8);
        assert_eq!(id_field.bit_counts.len(), id_field.lenbits as usize);
        assert_eq!(id_field.bit_order, BitOrder::Lsb);

        let nested_value = analyzer.field_stats.get("value").unwrap();
        assert_eq!(nested_value.full_path, "nested.value");
        assert_eq!(nested_value.name, "value");
        assert_eq!(nested_value.depth, 1);
        assert_eq!(nested_value.count, 0);
        assert_eq!(nested_value.lenbits, 8);
        assert_eq!(nested_value.bit_counts.len(), nested_value.lenbits as usize);
        assert_eq!(nested_value.bit_order, BitOrder::Lsb); // inherited from parent
    }

    #[test]
    fn skips_group_based_on_conditions() {
        let yaml = r#"
version: '1.0'
root:
  type: group
  skip_if_not:
    - byte_offset: 0
      bit_offset: 0
      bits: 8
      value: 0x55
  fields:
    dummy: 8
"#;
        let schema = Schema::from_yaml(yaml).unwrap();
        let mut analyzer = SchemaAnalyzer::new(&schema);

        // Should process - matching magic
        analyzer.add_entry(&[0x55]);
        assert_eq!(analyzer.field_stats.get("dummy").unwrap().count, 1);

        // Should skip - non-matching magic
        analyzer.add_entry(&[0xAA]);
        assert_eq!(analyzer.field_stats.get("dummy").unwrap().count, 1);

        // Should process - matching magic
        analyzer.add_entry(&[0x55]);
        assert_eq!(analyzer.field_stats.get("dummy").unwrap().count, 2);
    }

    #[test]
    fn skips_field_based_on_conditions() {
        let yaml = r#"
version: '1.0'
root:
  type: group
  fields:
    header:
      type: field
      bits: 7
      skip_if_not:
        - byte_offset: 0
          bit_offset: 0
          bits: 1
          value: 1
"#;
        let schema = Schema::from_yaml(yaml).unwrap();
        let mut analyzer = SchemaAnalyzer::new(&schema);

        // First bit 1 - processes
        analyzer.add_entry(&[0b10000000]);
        assert_eq!(analyzer.field_stats.get("header").unwrap().count, 1);

        // First bit 0 - skips
        analyzer.add_entry(&[0b00000000]);
        assert_eq!(analyzer.field_stats.get("header").unwrap().count, 1);
    }
}
