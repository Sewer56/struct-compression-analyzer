//! # Bit-Packed Structure Analysis Schema
//!
//! This module provides a structured way to define and analyze bit-packed data formats.
//!
//! The schema allows specifying:
//!
//! - Root structure definition
//! - Conditional offsets for data alignment
//! - Analysis configuration for comparing field groupings
//!
//! ## Public API
//!
//! ### Main Types
//!
//! - [Schema]: Main schema definition containing root structure and analysis configuration
//! - [Group]: Represents a group of fields or nested groups
//! - [FieldDefinition]: Defines a field or nested group
//! - [AnalysisConfig]: Configuration for analysis operations
//!
//! ### Public Methods
//!
//! - [`Schema::from_yaml()`]: Parse schema from YAML string
//! - [`Schema::load_from_file()`]: Load and parse schema from file path
//!
//! ### Group Component Methods
//!
//! - [`GroupComponentArray::get_bits()`]: Get number of bits to read from field
//!
//! ### Comparison Types
//!
//! - [SplitComparison]: Configuration for comparing two field group layouts
//! - [CustomComparison]: Configuration for comparing custom field group transformations
//!
//! ### Group Components
//!
//! - [GroupComponent]: Enum representing different types of group components
//!   - [GroupComponentArray]: Array of field values
//!   - [GroupComponentStruct]: Structured group of components
//!   - [GroupComponentPadding]: Padding bits
//!   - [GroupComponentSkip]: Skip bits
//!
//! ### Error Handling
//!
//! - [SchemaError]: Error types for schema parsing and validation
//!
//! ## Main Components
//!
//! - **Schema**: The root configuration containing:
//!   - Version
//!   - Metadata
//!   - Root group definition
//!   - Bit order configuration
//!   - Conditional offsets
//!   - Analysis configuration
//!
//! - **Group**: Hierarchical structure representing:
//!   - Group description
//!   - Nested fields/components
//!   - Bit order inheritance
//!   - Skip conditions
//!
//! - **FieldDefinition**: Represents either a:
//!   - [Field]: Single field with bit properties
//!   - [Group]: Nested group of fields
//!
//! ## Example Usage
//!
//! ```rust no_run
//! use struct_compression_analyzer::schema::*;
//! use std::path::Path;
//!
//! let yaml = r#"
//! version: '1.0'
//! metadata: { name: Test }
//! root: { type: group, fields: {} }
//! "#;
//!
//! // Load schema from YAML
//! let schema_from_file = Schema::load_from_file(Path::new("schema.yaml")).unwrap();
//! let schema_from_str = Schema::from_yaml(&yaml).unwrap();
//! ```

use indexmap::IndexMap;
use serde::Deserialize;
use std::path::Path;

use crate::analyzer::{AnalyzerFieldState, CompressionOptions};

/// Represents the complete schema configuration for a bit-packed structure to analyze.
///
/// The schema defines the layout and structure of the bit-packed data format.
/// It includes versioning, metadata, bit order configuration, and the root group definition.
///
/// # Examples
///
/// ```rust no_run
/// use struct_compression_analyzer::schema::Schema;
/// use std::path::Path;
///
/// let schema = Schema::load_from_file(Path::new("schema.yaml")).unwrap();
/// ```
#[derive(Debug, Deserialize, Default)]
pub struct Schema {
    /// Schema version. Currently only `1.0` is supported
    pub version: String,
    /// Contains user-provided metadata about the schema
    #[serde(default)]
    pub metadata: Metadata,
    /// Determines whether the bytes are read from the most significant bit (MSB)
    /// or least significant bit (LSB) first.
    ///
    /// - `Msb`: First bit is the high bit (7)
    /// - `Lsb`: First bit is the low bit (0)
    #[serde(default)]
    pub bit_order: BitOrder,
    /// Conditional offsets for the schema
    #[serde(default)]
    pub conditional_offsets: Vec<ConditionalOffset>,
    /// Configuration for analysis operations and output grouping
    #[serde(default)]
    pub analysis: AnalysisConfig,
    /// The root group of the schema
    pub root: Group,
}

/// Metadata about the schema
///
/// Contains user-provided information about the schema's purpose and structure.
#[derive(Clone, Debug, Deserialize, Default)]
pub struct Metadata {
    /// Name of the schema
    #[serde(default)]
    pub name: String,
    /// Description of the schema
    #[serde(default)]
    pub description: String,
}

/// Configuration for analysis operations and output grouping.
///
/// Defines how field groups should be compared and analyzed between each other,
/// to find the most optimal bit layout to use for the data.
#[derive(Debug, Deserialize, Default)]
pub struct AnalysisConfig {
    /// Compare structural equivalence between different field groups. Each comparison
    /// verifies that the compared groups have identical total bits and field structure.
    ///
    /// # Example
    /// ```yaml
    /// split_groups:
    ///   - name: colors
    ///     group_1: [colors]                       # Original interleaved (structure of array) RGB layout
    ///     group_2: [color_r, color_g, color_b]    # array of structure layout (e.g. RRRGGGBBB)
    ///     description: Compare compression ratio of original interleaved format against grouping of colour components.
    /// ```
    #[serde(default)]
    pub split_groups: Vec<SplitComparison>,

    /// Compare arbitrary field groups defined through custom transformations.
    /// Each comparison defines a baseline and one or more comparison groups
    /// that should be structurally equivalent but may have different bit layouts.
    ///
    /// # Example: Converting 7-bit colors to 8-bit
    ///
    /// ```yaml
    /// compare_groups:
    /// - name: convert_7_to_8_bit
    ///   description: "Adjust 7-bit color channel to 8-bit by appending a padding bit."
    ///   baseline: # R, R, R
    ///     - { type: array, field: color7 } # reads all '7-bit' colours from input
    ///   comparisons:
    ///     padded_8bit: # R+0, R+0, R+0
    ///       - type: struct
    ///         fields:
    ///           - { type: field, field: color7 } # reads 1 '7-bit' colour from input
    ///           - { type: padding, bits: 1, value: 0 } # appends 1 padding bit
    /// ```
    #[serde(default)]
    pub compare_groups: Vec<CustomComparison>,
}

/// Parameters for estimating compression size
#[derive(Debug, Deserialize, Clone)]
pub struct CompressionEstimationParams {
    /// Multiplier for LZ matches in size estimation (default: 0.375)
    #[serde(default = "default_lz_match_multiplier")]
    pub lz_match_multiplier: f64,
    /// Multiplier for entropy in size estimation (default: 1.0)
    #[serde(default = "default_entropy_multiplier")]
    pub entropy_multiplier: f64,
}

impl CompressionEstimationParams {
    pub fn new(options: &CompressionOptions) -> Self {
        Self {
            lz_match_multiplier: options.lz_match_multiplier,
            entropy_multiplier: options.entropy_multiplier,
        }
    }
}

/// Configuration for comparing field groups
#[derive(Debug, Deserialize)]
pub struct SplitComparison {
    /// Friendly name for this comparison.
    pub name: String,
    /// First group path to compare. This is the 'baseline'.
    pub group_1: Vec<String>,
    /// Second group path to compare. This is the group compared against the baseline (group_1).
    pub group_2: Vec<String>,
    /// Optional description of the comparison
    #[serde(default)]
    pub description: String,
    /// Compression estimation parameters for group 1
    #[serde(default)]
    pub compression_estimation_group_1: Option<CompressionEstimationParams>,
    /// Compression estimation parameters for group 2
    #[serde(default)]
    pub compression_estimation_group_2: Option<CompressionEstimationParams>,
}

/// Configuration for custom field group comparisons
#[derive(Debug, Deserialize)]
pub struct CustomComparison {
    /// Unique identifier for this comparison
    pub name: String,

    /// Baseline group definition
    pub baseline: Vec<GroupComponent>,

    /// Comparison group definitions with names
    pub comparisons: IndexMap<String, Vec<GroupComponent>>,

    /// Human-readable description
    #[serde(default)]
    pub description: String,
}

pub(crate) fn default_lz_match_multiplier() -> f64 {
    0.375
}

pub(crate) fn default_entropy_multiplier() -> f64 {
    1.0
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")] // Use "type" field as variant discriminant
pub enum GroupComponent {
    /// Array of field values
    #[serde(rename = "array")]
    Array(GroupComponentArray),

    /// Structured group of components
    #[serde(rename = "struct")]
    Struct(GroupComponentStruct),

    /// Padding bits.
    /// This should only be used from within structs.
    #[serde(rename = "padding")]
    Padding(GroupComponentPadding),

    /// Read the data from a field, once.
    /// This should only be used from within structs.
    #[serde(rename = "field")]
    Field(GroupComponentField),

    /// Skip a number of bits from a field.
    /// This should only be used from within structs.
    #[serde(rename = "skip")]
    Skip(GroupComponentSkip),
}

/// Reads all values of a single field until end of input.
/// i.e. `R0`, `R0`, `R0` etc. until all R0 values are read.
///
/// ```yaml
/// - { type: array, field: R } # reads all 'R' values from input
/// ```
///
/// This is read in a loop until no more bytes are written to output.  
/// Alternatively, you can read only some bits at a time using the `bits` field.  
///
/// ```yaml
/// - { type: array, field: R, offset: 2, bits: 4 } # read slice [2-6] for 'R' values from input
/// ```
///
/// Allowed properties:
///
/// - `offset`: Number of bits to skip before reading `bits`.
/// - `bits`: Number of bits to read (default: size of field)
/// - `field`: Field name
///
/// The `offset` and `bits` properties allow you to read a slice of a field.
/// Regardless of the slice read however, after each read is done, the stream will be advanced to the
/// next field.
///
/// Note: The `Array` type can be represented as `Struct` technically speaking, this is
/// actually a shorthand.
#[derive(Debug, Deserialize, Clone)]
pub struct GroupComponentArray {
    /// Name of the field to pull the data from.
    pub field: String,
    /// Offset in the field from which to read.
    #[serde(default)]
    pub offset: u32,
    /// The number of bits to read from the field.
    #[serde(default)]
    pub bits: u32,
    /// Multiplier for LZ matches in size estimation
    #[serde(default = "default_lz_match_multiplier")]
    pub lz_match_multiplier: f64,
    /// Multiplier for entropy in size estimation
    #[serde(default = "default_entropy_multiplier")]
    pub entropy_multiplier: f64,
}

impl Default for GroupComponentArray {
    fn default() -> Self {
        Self {
            field: String::new(),
            offset: 0,
            bits: 0,
            lz_match_multiplier: default_lz_match_multiplier(),
            entropy_multiplier: default_entropy_multiplier(),
        }
    }
}

impl GroupComponentArray {
    /// Retrieve the number of bits to read from the field.
    /// Either directly from the [`GroupComponentArray`] or if not specified, from the [`AnalyzerFieldState`].
    pub fn get_bits(&self, field: &AnalyzerFieldState) -> u32 {
        if self.bits == 0 {
            field.lenbits
        } else {
            self.bits
        }
    }
}

/// Structured group of components
///
/// ```yaml
/// - type: struct # R0 G0 B0. Repeats until no data written.
///   fields:
///     - { type: field, field: R } # reads 1 'R' value from input
///     - { type: field, field: G } # reads 1 'G' value from input
///     - { type: field, field: B } # reads 1 'B' value from input
/// ```
///
/// Allowed properties:
///
/// - `fields`: Array of field names
#[derive(Debug, Deserialize, Clone)]
pub struct GroupComponentStruct {
    /// Array of field names
    pub fields: Vec<GroupComponent>,
    /// Multiplier for LZ matches in size estimation
    #[serde(default = "default_lz_match_multiplier")]
    pub lz_match_multiplier: f64,
    /// Multiplier for entropy in size estimation
    #[serde(default = "default_entropy_multiplier")]
    pub entropy_multiplier: f64,
}

/// Padding bits  
/// This should only be used from within structs.
///
/// ```yaml
/// - { type: padding, bits: 4, value: 0 } # appends 4 padding bits
/// ```
///
/// Allowed properties:
///
/// - `bits`: Number of bits to insert
/// - `value`: Value to insert in those bits
#[derive(Debug, Deserialize, Clone)]
pub struct GroupComponentPadding {
    /// Number of bits to insert
    pub bits: u8,
    /// Value to insert in those bits
    #[serde(default)]
    pub value: u8,
}

/// Skip a number of bits from a field.
/// This should only be used from within structs.
///
/// ```yaml
/// - { type: skip, field: R, bits: 4 } # skips 4 bits from the 'R' field
/// ```
///
/// Allowed properties:
///
/// - `field`: Field name
/// - `bits`: Number of bits to skip
#[derive(Debug, Deserialize, Clone)]
pub struct GroupComponentSkip {
    /// Name of the field to skip bits from.
    pub field: String,
    /// Number of bits to skip from the field.
    pub bits: u32,
}

/// Read the data from a field, once.
/// This should only be used from within structs.
///
/// ```yaml
/// - { type: field, field: R } # reads 1 'R' value from input
/// ```
///
/// Allowed properties:
///
/// - `field`: Field name
/// - `bits`: Number of bits to read (default: size of field)
#[derive(Debug, Deserialize, Clone)]
pub struct GroupComponentField {
    /// Name of the field
    pub field: String,
    /// Number of bits to read from the field
    #[serde(default)]
    pub bits: u32,
}

impl GroupComponentField {
    /// Assign the number of bits to read from the field.
    /// Either keep value from [`GroupComponentField`] if manually specified, or override from the parameter.
    pub fn set_bits(&mut self, default: u32) {
        if self.bits == 0 {
            self.bits = default
        }
    }
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
    pub skip_if_not: Vec<Condition>,
    pub skip_frequency_analysis: bool,
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
                #[serde(default)]
                skip_if_not: Vec<Condition>,
                #[serde(default)]
                skip_frequency_analysis: bool,
            },
        }

        // The magic that allows for either shorthand or extended notation
        match FieldRepr::deserialize(deserializer)? {
            FieldRepr::Shorthand(size) => Ok(Field {
                bits: size,
                description: String::new(),
                bit_order: BitOrder::default(),
                skip_if_not: Vec::new(),
                skip_frequency_analysis: false,
            }),
            FieldRepr::Extended {
                bits,
                description,
                bit_order,
                skip_if_not,
                skip_frequency_analysis,
            } => Ok(Field {
                bits,
                description,
                bit_order,
                skip_if_not,
                skip_frequency_analysis,
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
    pub skip_if_not: Vec<Condition>,
    pub skip_frequency_analysis: bool,
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
            #[serde(default)]
            skip_if_not: Vec<Condition>,
            #[serde(default)]
            skip_frequency_analysis: bool,
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
            skip_if_not: group.skip_if_not,
            skip_frequency_analysis: group.skip_frequency_analysis,
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
///
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
    /// Bit order of the condition
    #[serde(default)]
    pub bit_order: BitOrder,
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
    /// Creates a new Schema from a YAML string.
    ///
    /// # Arguments
    /// * `content` - YAML string containing the schema definition
    ///
    /// # Returns
    /// * `Result<Self, SchemaError>` - Resulting schema or error
    pub fn from_yaml(content: &str) -> Result<Self, SchemaError> {
        let schema: Schema = serde_yaml::from_str(content)?;

        if schema.version != "1.0" {
            return Err(SchemaError::InvalidVersion);
        }

        Ok(schema)
    }

    /// Loads and parses a schema from a YAML file.
    ///
    /// # Arguments
    /// * `path` - Path to the schema YAML file
    ///
    /// # Returns
    /// * `Result<Self, SchemaError>` - Resulting schema or error
    pub fn load_from_file(path: &Path) -> Result<Self, SchemaError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_yaml(&content)
    }

    /// Collects a list of field (and group) paths in schema order.
    ///
    /// # Examples
    ///
    /// Given the following schema:
    ///
    /// ```yaml
    /// root:
    ///   type: group
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
    ///           type: array
    ///           field: R
    ///         g:
    ///           type: array
    ///           field: G
    ///         b:
    ///           type: array
    ///           field: B
    /// ```
    ///
    /// The resulting field paths would be:
    /// - "header"
    /// - "colors"
    /// - "colors.r"
    /// - "colors.g"
    /// - "colors.b"
    ///
    /// # Returns
    /// * `Vec<String>` - List of field paths in schema order
    pub fn ordered_field_and_group_paths(&self) -> Vec<String> {
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
bit_order: msb
"#;
            test_schema!(yaml, |schema: Schema| {
                assert_eq!(schema.version, "1.0");
                assert_eq!(schema.bit_order, BitOrder::Msb);
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
            bit_order: lsb
bit_order: msb
"#;
            test_schema!(yaml, |schema: Schema| {
                let field = match schema.root.fields.get("mode") {
                    Some(FieldDefinition::Field(f)) => f,
                    _ => panic!("Expected field"),
                };
                assert_eq!(field.bits, 3);
                assert_eq!(field.description, "Mode selector");
                assert_eq!(field.bit_order, BitOrder::Lsb);
                assert_eq!(schema.bit_order, BitOrder::Msb);
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
bit_order: msb
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
                assert_eq!(schema.bit_order, BitOrder::Msb);
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
bit_order: msb
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
bit_order: msb
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
                assert!(schema.analysis.split_groups.is_empty());
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

        #[test]
        fn supports_skip_if_not_conditions() {
            let yaml = r#"
version: '1.0'
metadata:
  name: Minimal Schema
root:
  type: group
  fields:
    header:
      type: group
      skip_if_not:
        - byte_offset: 0x00
          bit_offset: 0
          bits: 32
          value: 0x44445320
      fields:
        magic:
          type: field
          bits: 32
          skip_if_not:
            - byte_offset: 0x54
              bit_offset: 0
              bits: 32  
              value: 0x44583130
bit_order: msb
"#;

            let schema = Schema::from_yaml(yaml).unwrap();
            let header_group = match &schema.root.fields["header"] {
                FieldDefinition::Field(_field) => panic!("Expected group, got field"),
                FieldDefinition::Group(group) => group,
            };
            let magic_field = match &header_group.fields["magic"] {
                FieldDefinition::Field(field) => field,
                FieldDefinition::Group(_group) => panic!("Expected field, got group"),
            };

            // Test group-level conditions
            assert_eq!(header_group.skip_if_not.len(), 1);
            assert_eq!(header_group.skip_if_not[0].byte_offset, 0x00);
            assert_eq!(header_group.skip_if_not[0].value, 0x44445320);

            // Test field-level conditions
            assert_eq!(magic_field.skip_if_not.len(), 1);
            assert_eq!(magic_field.skip_if_not[0].byte_offset, 0x54);
            assert_eq!(magic_field.skip_if_not[0].value, 0x44583130);
            assert_eq!(schema.bit_order, BitOrder::Msb);
        }
    }

    mod split_compare_tests {
        use super::*;

        #[test]
        fn parses_basic_comparison() {
            let yaml = r#"
version: '1.0'
analysis:
  split_groups:
    - name: color_layouts
      group_1: [colors]
      group_2: [color_r, color_g, color_b]
      description: Compare interleaved vs planar layouts
      compression_estimation_group_1:
        lz_match_multiplier: 0.5
        entropy_multiplier: 1.2
      compression_estimation_group_2:
        lz_match_multiplier: 0.7
        entropy_multiplier: 1.5
root:
  type: group
  fields: {}
"#;

            let schema = Schema::from_yaml(yaml).unwrap();
            let comparisons = &schema.analysis.split_groups;

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

            // Check that compression estimation groups have values
            assert!(comparisons[0].compression_estimation_group_1.is_some());
            assert!(comparisons[0].compression_estimation_group_2.is_some());

            // Check the values
            let params1 = comparisons[0]
                .compression_estimation_group_1
                .as_ref()
                .unwrap();
            assert_eq!(params1.lz_match_multiplier, 0.5);
            assert_eq!(params1.entropy_multiplier, 1.2);

            let params2 = comparisons[0]
                .compression_estimation_group_2
                .as_ref()
                .unwrap();
            assert_eq!(params2.lz_match_multiplier, 0.7);
            assert_eq!(params2.entropy_multiplier, 1.5);
        }

        #[test]
        fn handles_minimal_comparison() {
            let yaml = r#"
version: '1.0'
analysis:
  split_groups:
    - name: basic
      group_1: [a]
      group_2: [b]
root:
  type: group
  fields: {}
"#;

            let schema = Schema::from_yaml(yaml).unwrap();
            let comparisons = &schema.analysis.split_groups;

            assert_eq!(comparisons.len(), 1);
            assert_eq!(comparisons[0].name, "basic");
            assert!(comparisons[0].description.is_empty());
            // Check that compression estimation groups are None when not specified
            assert!(comparisons[0].compression_estimation_group_1.is_none());
            assert!(comparisons[0].compression_estimation_group_2.is_none());
        }
    }

    mod group_compare_tests {
        use crate::schema::{GroupComponent, Schema};

        #[test]
        fn parses_custom_comparison() {
            let yaml = r#"
version: '1.0'
analysis:
  compare_groups:
    - name: convert_7_to_8_bit
      description: "Adjust 7-bit color channel to 8-bit by appending a padding bit."
      lz_match_multiplier: 0.45
      entropy_multiplier: 1.1
      baseline: # R, R, R
        - type: array
          field: color7
          bits: 7
          lz_match_multiplier: 0.5
          entropy_multiplier: 1.2
      comparisons:
        padded_8bit:
          - type: struct # R+0, R+0, R+0
            lz_match_multiplier: 0.6
            entropy_multiplier: 1.3
            fields:
              - { type: field, field: color7, bits: 7 } 
              - { type: padding, bits: 1, value: 0 } 
              - { type: skip, field: color7, bits: 0 } 
root:
  type: group
  fields: {}
"#;

            let schema = Schema::from_yaml(yaml).unwrap();
            let comparisons = &schema.analysis.compare_groups;

            assert_eq!(comparisons.len(), 1);
            assert_eq!(comparisons[0].name, "convert_7_to_8_bit");

            // Verify baseline
            let baseline = &comparisons[0].baseline;
            assert_eq!(baseline.len(), 1);
            match baseline.first().unwrap() {
                GroupComponent::Array(array) => {
                    assert_eq!(array.field, "color7");
                    assert_eq!(array.bits, 7);
                    // Verify the array component's multipliers
                    assert_eq!(array.lz_match_multiplier, 0.5);
                    assert_eq!(array.entropy_multiplier, 1.2);
                }
                _ => unreachable!("Expected an array type"),
            }

            // Verify comparisons
            let comps = &comparisons[0].comparisons;
            assert_eq!(comps.len(), 1);
            assert!(comps.contains_key("padded_8bit"));

            let padded = &comps["padded_8bit"];
            assert_eq!(padded.len(), 1);
            match padded.first().unwrap() {
                GroupComponent::Struct(group) => {
                    // Verify the struct component's multipliers
                    assert_eq!(group.lz_match_multiplier, 0.6);
                    assert_eq!(group.entropy_multiplier, 1.3);
                    assert_eq!(group.fields.len(), 3);

                    // Assert fields
                    match &group.fields[0] {
                        GroupComponent::Field(field) => {
                            assert_eq!(field.field, "color7");
                            assert_eq!(field.bits, 7);
                        }
                        _ => unreachable!("Expected a field type"),
                    }
                    match &group.fields[1] {
                        GroupComponent::Padding(padding) => {
                            assert_eq!(padding.bits, 1);
                            assert_eq!(padding.value, 0);
                        }
                        _ => unreachable!("Expected a padding type"),
                    }
                    match &group.fields[2] {
                        GroupComponent::Skip(skip) => {
                            assert_eq!(skip.bits, 0);
                        }
                        _ => unreachable!("Expected a skip type"),
                    }
                }
                _ => unreachable!("Expected a struct type"),
            }
        }

        #[test]
        fn rejects_invalid_custom_comparison() {
            let yaml = r#"

version: '1.0'
root:
  type: group
  fields: {}
analysis:
  compare_groups:
    - name: missing_fields
      group_1: [field_a]
"#;

            let result = Schema::from_yaml(yaml);
            assert!(result.is_err());
        }

        #[test]
        fn preserves_comparison_order() {
            let yaml = r#"
version: '1.0'
analysis:
  compare_groups:
    - name: bit_expansion
      description: "Test multiple comparison order preservation"
      baseline:
        - { type: array, field: original }
      comparisons:
        comparison_c:
          - { type: padding, bits: 1 }
        comparison_a: 
          - { type: padding, bits: 2 }
        comparison_b:
          - { type: padding, bits: 3 }
root:
  type: group
  fields: {}
"#;

            let schema = Schema::from_yaml(yaml).unwrap();
            let comparison = &schema.analysis.compare_groups[0];

            // Verify IndexMap preserves insertion order
            let keys: Vec<&str> = comparison.comparisons.keys().map(|s| s.as_str()).collect();
            assert_eq!(keys, vec!["comparison_c", "comparison_a", "comparison_b"]);

            // Verify basic parsing
            assert_eq!(comparison.name, "bit_expansion");
            assert_eq!(
                comparison.description,
                "Test multiple comparison order preservation"
            );
            assert_eq!(comparison.comparisons.len(), 3);
        }

        #[test]
        fn handles_minimal_custom_comparison() {
            let yaml = r#"
version: '1.0'
analysis:
  compare_groups:
    - name: minimal_test
      baseline: 
        - { type: array, field: test_field, bits: 8 } 
      comparisons:
        simple:
          - { type: array, field: test_field, bits: 8 } 
root:
  type: group
  fields: {}
"#;

            let schema = Schema::from_yaml(yaml).unwrap();
            let comparisons = &schema.analysis.compare_groups;

            assert_eq!(comparisons.len(), 1);
            assert_eq!(comparisons[0].name, "minimal_test");
            assert!(comparisons[0].description.is_empty());
        }
    }
}
