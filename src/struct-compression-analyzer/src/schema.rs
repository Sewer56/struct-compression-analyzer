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
