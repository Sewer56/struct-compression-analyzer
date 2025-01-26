//! # Bit-Packed Structure Analysis Schema
//!
//! Defines the schema for analyzing bit-packed data structures with nested groupings
//! and analysis configurations.
//!
//! ## Schema Format Documentation
//!
//! See `format-schema.md` in the `struct-compression-analyzer` repository root for complete YAML format details,
//! including examples and usage patterns.

use serde::Deserialize;
use std::{collections::HashMap, path::Path};

#[derive(Debug, Deserialize)]
pub struct Schema {
    pub version: String,
    pub metadata: Metadata,
    #[serde(default)]
    pub analysis: AnalysisConfig,
    pub fields: HashMap<String, FieldDefinition>,
}

#[derive(Debug, Deserialize)]
pub struct Metadata {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct AnalysisConfig {
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
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum FieldDefinition {
    Basic(BasicField),
    Group(Group),
}

/// Single field definition with direct bit mapping
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BasicField {
    /// Inclusive bit range [start, end] (0-based index)
    ///
    /// # Examples
    /// - `[0, 0]`: Single bit field
    /// - `[1, 4]`: 4-bit field (bits 1-4 inclusive)
    /// - `[8, 15]`: Full byte (bits 8-15)
    pub bits: (u32, u32),

    /// Documentation for the field's purpose and usage
    #[serde(default)]
    pub description: String,

    /// Bit interpretation order within the field
    ///
    /// # Behavior
    /// - `Msb` (default): Treats first bit as most significant
    ///   - `[0, 2]` => 0b100 = 4
    /// - `Lsb`: Treats first bit as least significant
    ///   - `[0, 2]` => 0b001 = 1
    #[serde(default)]
    #[serde(rename = "bit_order")]
    pub bit_order: BitOrder,
}

/// Group of related fields or components
#[derive(Debug, Deserialize)]
pub struct Group {
    /// Total bit range covered by all group components (inclusive)
    ///
    /// # Notes
    /// - For nested groups: Must encompass all child fields
    /// - For flat components: Should match total component range
    /// - Uses big-endian byte order (bits 0-7 in first byte)
    pub bits: (u32, u32),

    /// Human-readable group description
    #[serde(default)]
    pub description: String,

    /// Nested field definitions for hierarchical structures
    ///
    /// # Example
    /// ```yaml
    /// fields:
    ///   colors:
    ///     type: group
    ///     fields:
    ///       red: { bits: [0, 4] }
    ///       green: { bits: [5, 9] }
    /// ```
    #[serde(default)]
    pub fields: HashMap<String, FieldDefinition>,

    /// Flat component definitions with individual bit ranges
    ///
    /// # Example
    /// ```yaml
    /// components:
    ///   flag0: [0, 0]
    ///   flag1: [1, 1]
    /// ```
    #[serde(default)]
    pub components: HashMap<String, (u32, u32)>,
}

#[derive(Debug, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BitOrder {
    #[default]
    Msb,
    Lsb,
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

#[cfg(test)]
mod tests {
    use super::*;

    // Helper macro for testing schema parsing
    macro_rules! test_schema {
        ($yaml:expr, $test_fn:expr) => {
            let schema = Schema::from_yaml($yaml).unwrap();
            $test_fn(schema);
        };
    }

    // Version Section Tests
    mod version_tests {
        use super::*;

        #[test]
        fn test_valid_version() {
            let yaml = r#"version: '1.0'"#;
            assert!(Schema::from_yaml(yaml).is_ok());
        }

        #[test]
        fn test_invalid_version() {
            let yaml = r#"version: '2.0'"#;
            match Schema::from_yaml(yaml) {
                Err(SchemaError::InvalidVersion) => (),
                _ => panic!("Should reject invalid version"),
            }
        }
    }

    // Metadata Section Tests
    mod metadata_tests {
        use super::*;

        #[test]
        fn test_metadata_parsing() {
            let yaml = r#"
                version: '1.0'
                metadata:
                  name: BC7 Mode4
                  description: Test description
                fields: {}
            "#;

            test_schema!(yaml, |schema: Schema| {
                assert_eq!(schema.metadata.name, "BC7 Mode4");
                assert_eq!(schema.metadata.description, "Test description");
            });
        }

        #[test]
        fn test_optional_description() {
            let yaml = r#"
                version: '1.0'
                metadata:
                  name: Minimal Metadata
                fields: {}
            "#;

            test_schema!(yaml, |schema: Schema| {
                assert_eq!(schema.metadata.description, "");
            });
        }
    }

    // Analysis Section Tests
    mod analysis_tests {
        use super::*;

        #[test]
        fn test_group_by_config() {
            let yaml = r#"
                version: '1.0'
                metadata: { name: Test }
                analysis:
                  group_by:
                    - field: partition
                      description: Partition grouping
                      display:
                        format: "Part %02d"
                        labels: { 0: "None", 255: "Max" }
                    - field: colors.r.R0
                fields: {}
            "#;

            test_schema!(yaml, |schema: Schema| {
                let groups = &schema.analysis.group_by;
                assert_eq!(groups.len(), 2);

                let first = &groups[0];
                assert_eq!(first.field, "partition");
                assert_eq!(first.description, "Partition grouping");
                assert_eq!(first.display.format, "Part %02d");
                assert_eq!(first.display.labels["0"], "None");
                assert_eq!(first.display.labels["255"], "Max");

                let second = &groups[1];
                assert!(second.display.format.is_empty());
                assert!(second.display.labels.is_empty());
            });
        }
    }

    // Fields Section Tests
    mod fields_tests {
        use super::*;

        #[test]
        fn test_basic_field() {
            let yaml = r#"
                version: '1.0'
                metadata: { name: Test }
                fields:
                  mode:
                    bits: [0, 0]
                    description: Mode bit
                    bit_order: lsb
            "#;

            test_schema!(yaml, |schema: Schema| {
                let field = match &schema.fields["mode"] {
                    FieldDefinition::Basic(b) => b,
                    _ => panic!("Expected basic field"),
                };
                assert_eq!(field.bits, (0, 0));
                assert_eq!(field.description, "Mode bit");
                assert_eq!(field.bit_order, BitOrder::Lsb);
            });
        }

        #[test]
        fn test_nested_group() {
            let yaml = r#"
                version: '1.0'
                metadata: { name: Test }
                fields:
                  colors:
                    type: group
                    bits: [5, 28]
                    fields:
                      r:
                        type: group
                        components:
                          R0: [5, 8]
            "#;

            test_schema!(yaml, |schema: Schema| {
                let colors = match &schema.fields["colors"] {
                    FieldDefinition::Group(g) => g,
                    _ => panic!("Expected group"),
                };
                assert_eq!(colors.bits, (5, 28));

                let r = match &colors.fields["r"] {
                    FieldDefinition::Group(g) => g,
                    _ => panic!("Expected subgroup"),
                };
                assert_eq!(r.components["R0"], (5, 8));
            });
        }

        #[test]
        fn test_flat_group() {
            let yaml = r#"
                version: '1.0'
                metadata: { name: Test }
                fields:
                  p_bits:
                    type: group
                    components:
                      P0: [77, 77]
                      P1: [78, 78]
            "#;

            test_schema!(yaml, |schema: Schema| {
                let pbits = match &schema.fields["p_bits"] {
                    FieldDefinition::Group(g) => g,
                    _ => panic!("Expected group"),
                };
                assert_eq!(pbits.components["P0"], (77, 77));
                assert_eq!(pbits.components["P1"], (78, 78));
            });
        }
    }

    // Full File Test (Complete Example from Documentation)
    #[test]
    fn test_full_schema_example() {
        let yaml = r#"
            version: '1.0'
            metadata:
              name: BC1 Mode0 Block
              description: Analysis schema for Mode0 packed color structure

            analysis:
              group_by:
                - field: partition
                  description: Results grouped by partition value
                  display:
                    format: "Partition %d"

            fields:
              mode:
                bits: [0, 0]
                description: Mode bit

              partition:
                bits: [1, 4]
                bit_order: lsb

              colors:
                type: group
                description: All color components
                fields:
                  r:
                    type: group
                    bits: [5, 28]
                    components:
                      R0: [5, 8]
                      R1: [9, 12]

              p_bits:
                type: group
                bits: [77, 82]
                components:
                  P0: [77, 77]
                  P1: [78, 78]
        "#;

        test_schema!(yaml, |schema: Schema| {
            // Verify metadata
            assert_eq!(schema.metadata.name, "BC1 Mode0 Block");
            assert_eq!(
                schema.metadata.description,
                "Analysis schema for Mode0 packed color structure"
            );

            // Verify analysis config
            assert_eq!(schema.analysis.group_by[0].field, "partition");
            assert_eq!(schema.analysis.group_by[0].display.format, "Partition %d");

            // Verify fields
            let mode = match &schema.fields["mode"] {
                FieldDefinition::Basic(b) => b,
                _ => panic!("Expected basic field"),
            };
            assert_eq!(mode.bits, (0, 0));

            let colors = match &schema.fields["colors"] {
                FieldDefinition::Group(g) => g,
                _ => panic!("Expected group"),
            };
            assert_eq!(colors.description, "All color components");

            let pbits = match &schema.fields["p_bits"] {
                FieldDefinition::Group(g) => g,
                _ => panic!("Expected group"),
            };
            assert_eq!(pbits.bits, (77, 82));
        });
    }

    // Error Handling Tests
    mod error_tests {
        use super::*;

        #[test]
        fn test_invalid_group_type() {
            let yaml = r#"
                version: '1.0'
                metadata: { name: Test }
                fields:
                  invalid:
                    type: invalid
                    bits: [0, 0]
            "#;

            assert!(Schema::from_yaml(yaml).is_err());
        }

        #[test]
        fn test_missing_required_fields() {
            let yaml = r#"
                version: '1.0'
                metadata: { name: Test }
            "#;

            assert!(Schema::from_yaml(yaml).is_err());
        }

        #[test]
        fn test_file_loading() -> Result<(), SchemaError> {
            let temp_file = std::env::temp_dir().join("test_schema.yaml");
            std::fs::write(&temp_file, include_str!("../../../format-schema.md"))?;

            let result = Schema::load_from_file(&temp_file);
            std::fs::remove_file(temp_file)?;
            assert!(result.is_ok());
            Ok(())
        }
    }

    // Edge Case Tests
    mod edge_cases {
        use super::*;

        #[test]
        fn test_minimal_valid_schema() {
            let yaml = r#"
                version: '1.0'
                metadata: { name: Minimal }
                fields: {}
            "#;
            assert!(Schema::from_yaml(yaml).is_ok());
        }

        #[test]
        fn test_empty_group() {
            let yaml = r#"
                version: '1.0'
                metadata: { name: Test }
                fields:
                  empty_group:
                    type: group
            "#;
            assert!(Schema::from_yaml(yaml).is_err());
        }
    }
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
