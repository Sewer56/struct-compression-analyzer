//! # Bit-Packed Structure Analysis Schema
//!
//! Defines the schema for analyzing bit-packed data structures with nested groupings.

use indexmap::IndexMap;
use serde::Deserialize;
use std::{collections::HashMap, path::Path};

#[derive(Debug, Deserialize, Default)]
pub struct Schema {
    /// Schema version. Currently only `1.0` is supported
    pub version: String,
    /// Contains user-provided metadata about the schema
    #[serde(default)]
    pub metadata: Metadata,
    /// Conditional offsets for the schema
    #[serde(default)]
    pub conditional_offsets: Vec<ConditionalOffset>,
    /// Configuration for analysis operations and output grouping
    #[serde(default)]
    pub analysis: AnalysisConfig,
    /// The root group of the schema
    pub root: Group,
}

/// Contains user-provided metadata about the schema
#[derive(Clone, Debug, Deserialize, Default)]
pub struct Metadata {
    /// Name of the schema
    #[serde(default)]
    pub name: String,
    /// Description of the schema
    #[serde(default)]
    pub description: String,
}

/// Configuration for analysis operations and output grouping
#[derive(Debug, Deserialize, Default)]
pub struct AnalysisConfig {
    /// List of field groupings to use when organizing analysis results
    ///
    /// # Example
    /// ```yaml
    /// analysis:
    ///   group_by_field:
    ///     - field: partition
    ///       description: Results by partition value
    ///     - field: colors.r.R0
    ///       display:
    ///         format: "R Component %02X"
    /// ```
    #[serde(default)]
    pub group_by_field: Vec<GroupByFieldConfig>,

    /// Compare structural equivalence between different field groups. Each comparison
    /// verifies that the compared groups have identical total bits and field structure.
    ///
    /// # Example
    /// ```yaml
    /// compare_groups:
    ///   - name: colors
    ///     group_1: [colors]                       # Original interleaved (structure of array) RGB layout
    ///     group_2: [color_r, color_g, color_b]    # array of structure layout (e.g. RRRGGGBBB)
    ///     description: Compare compression ratio of original interleaved format against grouping of colour components.
    /// ```
    #[serde(default)]
    pub compare_groups: Vec<GroupComparison>,
}

/// Configuration for comparing field groups
#[derive(Debug, Deserialize)]
pub struct GroupComparison {
    /// Friendly name for this comparison.
    pub name: String,
    /// First group path to compare. This is the 'baseline'.
    pub group_1: Vec<String>,
    /// Second group path to compare. This is the group compared against the baseline (group_1).
    pub group_2: Vec<String>,
    /// Optional description of the comparison
    #[serde(default)]
    pub description: String,
}

/// Configuration for grouping analysis results by field values
#[derive(Debug, Deserialize)]
pub struct GroupByFieldConfig {
    /// Field path to group by (supports nested dot notation)
    ///
    /// # Examples
    /// - `"partition"`: Top-level field
    /// - `"colors.r.R0"`: Nested group component
    pub field: String,

    /// Descriptive text for analysis output headers
    ///
    /// # Example
    /// "Results grouped by color component values"
    #[serde(default)]
    pub description: String,

    /// Display configuration for group values
    ///
    /// Combines format string and labels to present values meaningfully:
    /// ```yaml
    /// display:
    ///   format: "Mode %02d"
    ///   labels:
    ///     0: "Disabled"
    ///     255: "Special"
    /// ```
    #[serde(default)]
    pub display: DisplayConfig,
}

/// Display configuration for analysis groupings
#[derive(Debug, Deserialize, Default)]
pub struct DisplayConfig {
    /// Format string using printf-style syntax for displaying group values
    ///
    /// Common format specifiers:
    /// - `%d`: Decimal integer (e.g., 42)
    /// - `%x`: Lowercase hexadecimal (e.g., 2a)
    /// - `%X`: Uppercase hexadecimal (e.g., 2A)
    /// - `%02d`: Zero-padded decimal (e.g., 02)
    /// - `%s`: String representation (requires labels mapping)
    ///
    /// # Examples
    ///
    /// ```yaml
    /// display:
    ///   format: "Version %04X"
    ///   labels:
    ///     0: "Legacy"
    ///     1: "Current"
    /// ```
    #[serde(default)]
    pub format: String,

    /// Value-to-label mappings for human-readable display
    ///
    /// ```yaml
    /// labels:
    ///   0: "Disabled"
    ///   1: "Enabled"
    ///   2: "Partial"
    /// ```
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

/// Allows us to define a nested item as either a field or group
#[derive(Debug, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum FieldDefinition {
    Field(Field),
    Group(Group),
}

/// A single field definition
#[derive(Debug)]
pub struct Field {
    pub bits: u32,
    pub description: String,
    pub bit_order: BitOrder,
}

impl<'de> Deserialize<'de> for Field {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum FieldRepr {
            Shorthand(u32),
            Extended {
                bits: u32,
                #[serde(default)]
                description: String,
                #[serde(default)]
                #[serde(rename = "bit_order")]
                bit_order: BitOrder,
            },
        }

        // The magic that allows for either shorthand or extended notation
        match FieldRepr::deserialize(deserializer)? {
            FieldRepr::Shorthand(size) => Ok(Field {
                bits: size,
                description: String::new(),
                bit_order: BitOrder::default(),
            }),
            FieldRepr::Extended {
                bits,
                description,
                bit_order,
            } => Ok(Field {
                bits,
                description,
                bit_order,
            }),
        }
    }
}

/// Group of related fields or components
///
/// Represents a logical grouping of fields in the bit-packed structure.
/// Groups can contain both individual fields and nested sub-groups.
///
/// # Fields
/// - `_type`: Must be "group" (validated during parsing)
/// - `description`: Optional description of the group's purpose
/// - `fields`: Map of field names to their definitions (fields or sub-groups)
///
/// # Examples
/// ```yaml
/// root:
///   type: group
///   description: Main structure
///   fields:
///     header:
///       type: group
///       fields:
///         mode: 2
///         partition: 4
///     colors:
///       type: group
///       fields:
///         r:
///           type: group
///           fields:
///             R0: 5
///             R1: 5
/// ```
#[derive(Debug, Default)]
pub struct Group {
    _type: String,
    pub description: String,
    pub fields: IndexMap<String, FieldDefinition>,
    /// Total bits calculated from children fields/groups
    pub bits: u32,
    /// The bit order of this group.
    /// Inherited by all the children unless explicitly overwritten.
    pub bit_order: BitOrder,
}

impl<'de> Deserialize<'de> for Group {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct GroupRepr {
            #[serde(rename = "type")]
            _type: String,
            #[serde(default)]
            description: String,
            #[serde(default)]
            bit_order: BitOrder,
            #[serde(default)]
            fields: IndexMap<String, FieldDefinition>,
        }

        let group = GroupRepr::deserialize(deserializer)?;
        if group._type != "group" {
            return Err(serde::de::Error::custom(format!(
                "Invalid group type: {} (must be 'group')",
                group._type
            )));
        }

        // Calculate total bits from children
        // This is recursive. Deserialize of child would have calculated this for the child.
        let bits = group
            .fields
            .values()
            .map(|fd| match fd {
                FieldDefinition::Field(f) => f.bits,
                FieldDefinition::Group(g) => g.bits,
            })
            .sum();

        // Create the group with its own bit_order
        let mut group = Group {
            _type: group._type,
            description: group.description,
            fields: group.fields,
            bits,
            bit_order: group.bit_order,
        };

        // Propagate bit_order to children if not explicitly set
        let bit_order = group.bit_order;
        propagate_bit_order(&mut group, bit_order);

        Ok(group)
    }
}

impl Group {
    /// Collects a list of field paths in schema order
    /// This includes both fields and groups
    fn collect_field_paths(&self, paths: &mut Vec<String>, parent_path: &str) {
        for (name, item) in &self.fields {
            match item {
                FieldDefinition::Field(_) => {
                    let full_path = if parent_path.is_empty() {
                        name
                    } else {
                        &format!("{}.{}", parent_path, name)
                    };
                    paths.push(full_path.clone());
                }
                FieldDefinition::Group(g) => {
                    let new_parent = if parent_path.is_empty() {
                        name
                    } else {
                        &format!("{}.{}", parent_path, name)
                    };
                    paths.push(new_parent.clone());
                    g.collect_field_paths(paths, new_parent);
                }
            }
        }
    }
}

/// Bit ordering specification for field values
///
/// Determines how bits are interpreted within a field:
/// - `Msb`: Most significant bit first (default)
/// - `Lsb`: Least significant bit first
///
/// # Examples
/// ```yaml
/// bit_order: msb  # Default, bits are read left-to-right
/// bit_order: lsb  # Bits are read right-to-left
/// ```
#[derive(Debug, Deserialize, Default, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum BitOrder {
    /// Not initialized. If not set down the road, defaults to [Msb](BitOrder::Msb)
    #[default]
    Default,
    Msb,
    Lsb,
}

impl BitOrder {
    pub fn get_with_default_resolve(self) -> BitOrder {
        if self == BitOrder::Default {
            BitOrder::Msb
        } else {
            self
        }
    }
}

/// Recursively propagates bit_order to child fields and groups
fn propagate_bit_order(group: &mut Group, parent_bit_order: BitOrder) {
    for (_, field_def) in group.fields.iter_mut() {
        match field_def {
            FieldDefinition::Field(field) => {
                // Only inherit if field has default bit_order
                if field.bit_order == BitOrder::Default {
                    field.bit_order = parent_bit_order;
                }
            }
            FieldDefinition::Group(child_group) => {
                // Only inherit if child group has default bit_order
                if child_group.bit_order == BitOrder::Default {
                    child_group.bit_order = parent_bit_order;
                }
                // Recursively propagate to nested groups
                propagate_bit_order(child_group, child_group.bit_order);
            }
        }
    }
}

/// Defines a single condition for offset selection
///
/// # Examples
///
/// ```yaml
/// byte_offset: 0x00
/// bit_offset: 0
/// bits: 32
/// value: 0x44445320  # DDS magic
/// ```
#[derive(Debug, PartialEq, Clone, serde::Deserialize)]
pub struct Condition {
    /// Byte offset from start of structure
    pub byte_offset: u64,
    /// Bit offset within the byte (0-7, left to right)
    pub bit_offset: u8,
    /// Number of bits to compare (1-32)
    pub bits: u8,
    /// Expected value in big-endian byte order
    pub value: u64,
}

/// Defines conditional offset selection rules
///
/// # Examples
///
/// ```yaml
/// - offset: 0x94  # BC7 data offset
///   conditions:
///     - byte_offset: 0x00
///       bit_offset: 0
///       bits: 32
///       value: 0x44445320
///     - byte_offset: 0x54
///       bit_offset: 0
///       bits: 32
///       value: 0x44583130
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct ConditionalOffset {
    /// Target offset to use if conditions match
    pub offset: u64,
    /// List of conditions that must all be satisfied
    pub conditions: Vec<Condition>,
}

#[derive(thiserror::Error, Debug)]
pub enum SchemaError {
    #[error("Invalid schema version (expected 1.0)")]
    InvalidVersion,
    #[error("YAML parsing error: {0}")]
    YamlError(#[from] serde_yaml::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid group type: {0} (must be 'group')")]
    InvalidGroupType(String),
}

impl Schema {
    pub fn from_yaml(content: &str) -> Result<Self, SchemaError> {
        let schema: Schema = serde_yaml::from_str(content)?;

        if schema.version != "1.0" {
            return Err(SchemaError::InvalidVersion);
        }

        Ok(schema)
    }

    pub fn load_from_file(path: &Path) -> Result<Self, SchemaError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_yaml(&content)
    }

    /// Returns a list of field paths in schema order
    /// This includes both fields and groups
    pub fn ordered_field_paths(&self) -> Vec<String> {
        let mut paths = Vec::new();
        self.root.collect_field_paths(&mut paths, "");
        paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! test_schema {
        ($yaml:expr, $test:expr) => {{
            let schema = Schema::from_yaml($yaml).expect("Failed to parse schema");
            $test(schema);
        }};
    }

    // Version Tests
    mod version_tests {
        use super::*;

        #[test]
        fn supports_version_10() {
            let yaml = r#"
version: '1.0'
metadata: { name: Test }
root: { type: group, fields: {} }
"#;
            test_schema!(yaml, |schema: Schema| {
                assert_eq!(schema.version, "1.0");
            });
        }

        #[test]
        fn rejects_unsupported_version() {
            let yaml = r#"
version: '2.0'
metadata: { name: Test }
root: { type: group, fields: {} }
"#;
            assert!(Schema::from_yaml(yaml).is_err());
        }
    }

    // Metadata Tests
    mod metadata_tests {
        use super::*;

        #[test]
        fn parses_full_metadata() {
            let yaml = r#"
version: '1.0'
metadata:
    name: BC7 Mode4
    description: Test description
root: { type: group, fields: {} }
"#;
            test_schema!(yaml, |schema: Schema| {
                assert_eq!(schema.metadata.name, "BC7 Mode4");
                assert_eq!(schema.metadata.description, "Test description");
            });
        }

        #[test]
        fn handles_empty_metadata() {
            let yaml = r#"
version: '1.0'
root: { type: group, fields: {} }
"#;
            test_schema!(yaml, |schema: Schema| {
                assert_eq!(schema.metadata.name, "");
                assert_eq!(schema.metadata.description, "");
            });
        }
    }

    // Analysis Section Tests
    mod analysis_tests {
        use super::*;

        #[test]
        fn supports_group_by_field_with_display_config() {
            let yaml = r#"
version: '1.0'
metadata: { name: Test }
analysis:
    group_by_field:
    - field: partition
      description: Results by partition value
      display:
        format: "%02d"
        labels: { 0: "None", 1: "Two", 255: "Max" }
    - field: R0
      description: Red component grouping (nested field)
      display:
        format: "%02X"
root: { type: group, fields: {} }
"#;
            test_schema!(yaml, |schema: Schema| {
                let groups = &schema.analysis.group_by_field;
                assert_eq!(groups.len(), 2);

                let first = &groups[0];
                assert_eq!(first.field, "partition");
                assert_eq!(first.description, "Results by partition value");
                assert_eq!(first.display.format, "%02d");
                assert_eq!(first.display.labels.get("0").unwrap(), "None");
                assert_eq!(first.display.labels.get("1").unwrap(), "Two");
                assert_eq!(first.display.labels.get("255").unwrap(), "Max");

                let second = &groups[1];
                assert_eq!(second.field, "R0");
                assert_eq!(second.description, "Red component grouping (nested field)");
                assert_eq!(second.display.format, "%02X");
                assert!(second.display.labels.is_empty());
            });
        }

        #[test]
        fn supports_multiple_format_specifiers() {
            let yaml = r#"
version: '1.0'
metadata: { name: Test }
analysis:
    group_by_field:
    - field: mode
      display:
        format: "%d"
    - field: color
      display:
        format: "%02x"
    - field: version
      display:
        format: "%X"
    - field: name
      display:
        format: "%s"
root: { type: group, fields: {} }
"#;
            test_schema!(yaml, |schema: Schema| {
                let groups = &schema.analysis.group_by_field;
                assert_eq!(groups.len(), 4);
                assert_eq!(groups[0].display.format, "%d"); // decimal
                assert_eq!(groups[1].display.format, "%02x"); // padded hex
                assert_eq!(groups[2].display.format, "%X"); // uppercase hex
                assert_eq!(groups[3].display.format, "%s"); // string
            });
        }
    }

    // Fields Section Tests
    mod fields_tests {
        use super::*;

        #[test]
        fn supports_shorthand_field() {
            let yaml = r#"
version: '1.0'
metadata: { name: Test }
root:
    type: group
    fields:
        mode: 2
        partition: 4
"#;
            test_schema!(yaml, |schema: Schema| {
                let mode = match schema.root.fields.get("mode") {
                    Some(FieldDefinition::Field(f)) => f,
                    _ => panic!("Expected field"),
                };
                assert_eq!(mode.bits, 2);

                let partition = match schema.root.fields.get("partition") {
                    Some(FieldDefinition::Field(f)) => f,
                    _ => panic!("Expected field"),
                };
                assert_eq!(partition.bits, 4);
            });
        }

        #[test]
        fn supports_extended_field() {
            let yaml = r#"
version: '1.0'
metadata: { name: Test }
root:
    type: group
    fields:
        mode:
            type: field
            bits: 3
            description: Mode selector
            bit_order: msb
"#;
            test_schema!(yaml, |schema: Schema| {
                let field = match schema.root.fields.get("mode") {
                    Some(FieldDefinition::Field(f)) => f,
                    _ => panic!("Expected field"),
                };
                assert_eq!(field.bits, 3);
                assert_eq!(field.description, "Mode selector");
                assert_eq!(field.bit_order, BitOrder::Msb);
            });
        }

        #[test]
        fn supports_nested_groups() {
            let yaml = r#"
version: '1.0'
metadata: { name: Test }
root:
    type: group
    fields:
        header:
            type: group
            fields:
                mode: 2
                partition: 4
        colors:
            type: group
            fields:
                r:
                    type: group
                    fields:
                        R0: 5
                        R1: 5
"#;
            test_schema!(yaml, |schema: Schema| {
                let header = match schema.root.fields.get("header") {
                    Some(FieldDefinition::Group(g)) => g,
                    _ => panic!("Expected group"),
                };
                assert_eq!(header.fields.len(), 2);

                let colors = match schema.root.fields.get("colors") {
                    Some(FieldDefinition::Group(g)) => g,
                    _ => panic!("Expected group"),
                };
                let r = match colors.fields.get("r") {
                    Some(FieldDefinition::Group(g)) => g,
                    _ => panic!("Expected group"),
                };
                assert_eq!(r.fields.len(), 2);
            });
        }

        #[test]
        fn calculates_group_bits_from_children() {
            let yaml = r#"                                                                                                                                                
 version: '1.0'                                                                                                                                                    
 root:                                                                                                                                                             
     type: group                                                                                                                                                   
     fields:                                                                                                                                                       
         a: 4                                                                                                                                                      
         b: 8                                                                                                                                                      
         subgroup:                                                                                                                                                 
             type: group                                                                                                                                           
             fields:                                                                                                                                               
                 c: 2                                                                                                                                              
                 d: 2                                                                                                                                              
 "#;
            test_schema!(yaml, |schema: Schema| {
                // Top level group should have 4 + 8 + (2+2) = 16 bits
                assert_eq!(schema.root.bits, 16);
                // Subgroup should have 2 + 2 = 4 bits
                match schema.root.fields.get("subgroup") {
                    Some(FieldDefinition::Group(g)) => assert_eq!(g.bits, 4),
                    _ => panic!("Expected subgroup"),
                }
            });
        }
    }

    // Bit Order Tests
    mod bit_order_tests {
        use super::*;

        #[test]
        fn inherits_bit_order_from_parent() {
            let yaml = r#"
version: '1.0'
root:
    type: group
    bit_order: lsb
    fields:
        a: 4
        b: 8
        subgroup:
            type: group
            fields:
                c: 2
                d: 2
"#;
            test_schema!(yaml, |schema: Schema| {
                // Check root fields
                match schema.root.fields.get("a") {
                    Some(FieldDefinition::Field(f)) => assert_eq!(f.bit_order, BitOrder::Lsb),
                    _ => panic!("Expected field"),
                }
                match schema.root.fields.get("b") {
                    Some(FieldDefinition::Field(f)) => assert_eq!(f.bit_order, BitOrder::Lsb),
                    _ => panic!("Expected field"),
                }

                // Check nested group and its fields
                match schema.root.fields.get("subgroup") {
                    Some(FieldDefinition::Group(g)) => {
                        assert_eq!(g.bit_order, BitOrder::Lsb);
                        match g.fields.get("c") {
                            Some(FieldDefinition::Field(f)) => {
                                assert_eq!(f.bit_order, BitOrder::Lsb)
                            }
                            _ => panic!("Expected field"),
                        }
                        match g.fields.get("d") {
                            Some(FieldDefinition::Field(f)) => {
                                assert_eq!(f.bit_order, BitOrder::Lsb)
                            }
                            _ => panic!("Expected field"),
                        }
                    }
                    _ => panic!("Expected subgroup"),
                }
            });
        }

        #[test]
        fn preserves_explicit_bit_order_in_children() {
            let yaml = r#"
version: '1.0'
root:
    type: group
    bit_order: lsb
    fields:
        a: 4
        b:
            type: field
            bits: 8
            bit_order: msb
        subgroup:
            type: group
            bit_order: msb
            fields:
                c: 2
                d: 2
"#;
            test_schema!(yaml, |schema: Schema| {
                // Check root fields
                match schema.root.fields.get("a") {
                    Some(FieldDefinition::Field(f)) => assert_eq!(f.bit_order, BitOrder::Lsb),
                    _ => panic!("Expected field"),
                }
                match schema.root.fields.get("b") {
                    Some(FieldDefinition::Field(f)) => assert_eq!(f.bit_order, BitOrder::Msb),
                    _ => panic!("Expected field"),
                }

                // Check nested group and its fields
                match schema.root.fields.get("subgroup") {
                    Some(FieldDefinition::Group(g)) => {
                        assert_eq!(g.bit_order, BitOrder::Msb);
                        match g.fields.get("c") {
                            Some(FieldDefinition::Field(f)) => {
                                assert_eq!(f.bit_order, BitOrder::Msb)
                            }
                            _ => panic!("Expected field"),
                        }
                        match g.fields.get("d") {
                            Some(FieldDefinition::Field(f)) => {
                                assert_eq!(f.bit_order, BitOrder::Msb)
                            }
                            _ => panic!("Expected field"),
                        }
                    }
                    _ => panic!("Expected subgroup"),
                }
            });
        }

        #[test]
        fn uses_default_bit_order_when_not_specified() {
            let yaml = r#"
version: '1.0'
root:
    type: group
    fields:
        a: 4
        b: 8
"#;
            test_schema!(yaml, |schema: Schema| {
                match schema.root.fields.get("a") {
                    Some(FieldDefinition::Field(f)) => assert_eq!(f.bit_order, BitOrder::Default),
                    _ => panic!("Expected field"),
                }
                match schema.root.fields.get("b") {
                    Some(FieldDefinition::Field(f)) => assert_eq!(f.bit_order, BitOrder::Default),
                    _ => panic!("Expected field"),
                }
            });
        }
    }

    // Edge Cases
    mod edge_cases {
        use super::*;

        #[test]
        fn accepts_minimal_valid_schema() {
            let yaml = r#"
version: '1.0'
root: { type: group, fields: {} }
"#;
            test_schema!(yaml, |schema: Schema| {
                assert_eq!(schema.version, "1.0");
                assert!(schema.root.fields.is_empty());
            });
        }

        #[test]
        fn handles_empty_analysis() {
            let yaml = r#"
version: '1.0'
metadata: { name: Test }
analysis: {}
root: { type: group, fields: {} }
"#;
            test_schema!(yaml, |schema: Schema| {
                assert!(schema.analysis.group_by_field.is_empty());
            });
        }

        #[test]
        fn handles_empty_display_config() {
            let yaml = r#"
version: '1.0'
metadata: { name: Test }
analysis:
    group_by_field:
    - field: test
      display: {}
root: { type: group, fields: {} }
"#;
            test_schema!(yaml, |schema: Schema| {
                let group = &schema.analysis.group_by_field[0];
                assert!(group.display.format.is_empty());
                assert!(group.display.labels.is_empty());
            });
        }
    }

    // Conditional Offset Tests
    mod conditional_offset_tests {
        use super::*;

        #[test]
        fn parses_basic_conditional_offset() {
            let yaml = r#"
version: '1.0'
metadata:
  name: Test Schema
conditional_offsets:
  - offset: 0x94
    conditions:
      - byte_offset: 0x00
        bit_offset: 0
        bits: 32
        value: 0x44445320  # DDS magic
      - byte_offset: 0x54
        bit_offset: 0
        bits: 32
        value: 0x44583130
root:
  type: group
  fields: {}
"#;

            let schema: Schema = serde_yaml::from_str(yaml).unwrap();
            assert_eq!(schema.conditional_offsets.len(), 1);

            let offset = &schema.conditional_offsets[0];
            assert_eq!(offset.offset, 0x94);
            assert_eq!(offset.conditions.len(), 2);

            let cond1 = &offset.conditions[0];
            assert_eq!(cond1.byte_offset, 0x00);
            assert_eq!(cond1.bit_offset, 0);
            assert_eq!(cond1.bits, 32);
            assert_eq!(cond1.value, 0x44445320);
        }

        #[test]
        fn handles_missing_optional_fields() {
            let yaml = r#"
version: '1.0'
metadata:
  name: Minimal Schema
root:
  type: group
  fields: {}
"#;

            let schema: Schema = serde_yaml::from_str(yaml).unwrap();
            assert!(schema.conditional_offsets.is_empty());
        }
    }

    mod group_compare_tests {
        use crate::schema::Schema;

        #[test]
        fn parses_basic_comparison() {
            let yaml = r#"
version: '1.0'
analysis:
  compare_groups:
    - name: color_layouts
      group_1: [colors]
      group_2: [color_r, color_g, color_b]
      description: Compare interleaved vs planar layouts
root:
  type: group
  fields: {}
"#;

            let schema = Schema::from_yaml(yaml).unwrap();
            let comparisons = &schema.analysis.compare_groups;

            assert_eq!(comparisons.len(), 1);
            assert_eq!(comparisons[0].name, "color_layouts");
            assert_eq!(comparisons[0].group_1, vec!["colors"]);
            assert_eq!(
                comparisons[0].group_2,
                vec!["color_r", "color_g", "color_b"]
            );
            assert_eq!(
                comparisons[0].description,
                "Compare interleaved vs planar layouts"
            );
        }

        #[test]
        fn handles_minimal_comparison() {
            let yaml = r#"
version: '1.0'
analysis:
  compare_groups:
    - name: basic
      group_1: [a]
      group_2: [b]
root:
  type: group
  fields: {}
"#;

            let schema = Schema::from_yaml(yaml).unwrap();
            let comparisons = &schema.analysis.compare_groups;

            assert_eq!(comparisons.len(), 1);
            assert_eq!(comparisons[0].name, "basic");
            assert!(comparisons[0].description.is_empty());
        }
    }
}
