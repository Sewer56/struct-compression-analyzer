//! # Bit-Packed Structure Analysis Schema
//!
//! Defines the schema for analyzing bit-packed data structures with nested groupings.

use indexmap::IndexMap;
use serde::Deserialize;
use std::{collections::HashMap, path::Path};

#[derive(Debug, Deserialize, Default)]
pub struct Schema {
    pub version: String,
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default)]
    pub analysis: AnalysisConfig,
    pub root: Group,
}

#[derive(Debug, Deserialize, Default)]
pub struct Metadata {
    #[serde(default)]
    pub name: String,
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
    ///   group_by:
    ///     - field: partition
    ///       description: Results by partition value
    ///     - field: colors.r.R0
    ///       display:
    ///         format: "R Component %02X"
    /// ```
    #[serde(default)]
    pub group_by: Vec<GroupByConfig>,
}

/// Configuration for grouping analysis results by field values
#[derive(Debug, Deserialize)]
pub struct GroupByConfig {
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

#[derive(Debug, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum FieldDefinition {
    Field(Field),
    Group(Group),
}

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
#[derive(Debug, Deserialize, Default, PartialEq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum BitOrder {
    #[default]
    Default,
    Msb,
    Lsb,
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
        fn supports_group_by_with_display_config() {
            let yaml = r#"
version: '1.0'
metadata: { name: Test }
analysis:
    group_by:
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
                let groups = &schema.analysis.group_by;
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
    group_by:
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
                let groups = &schema.analysis.group_by;
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
                assert!(schema.analysis.group_by.is_empty());
            });
        }

        #[test]
        fn handles_empty_display_config() {
            let yaml = r#"
version: '1.0'
metadata: { name: Test }
analysis:
    group_by:
    - field: test
      display: {}
root: { type: group, fields: {} }
"#;
            test_schema!(yaml, |schema: Schema| {
                let group = &schema.analysis.group_by[0];
                assert!(group.display.format.is_empty());
                assert!(group.display.labels.is_empty());
            });
        }
    }
}
