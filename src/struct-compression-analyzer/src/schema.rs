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

#[derive(Debug, Deserialize)]
pub struct GroupByConfig {
    pub field: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub display: DisplayConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct DisplayConfig {
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum FieldDefinition {
    Basic(BasicField),
    Group(Group),
}

#[derive(Debug, Deserialize)]
pub struct BasicField {
    pub bits: (u32, u32),
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    #[serde(rename = "bit_order")]
    pub bit_order: BitOrder,
}

#[derive(Debug, Deserialize)]
pub struct Group {
    #[serde(rename = "type")]
    pub group_type: String,
    pub bits: (u32, u32),
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub fields: HashMap<String, FieldDefinition>,
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
        let mut schema: Schema = serde_yaml::from_str(content)?;

        if schema.version != "1.0" {
            return Err(SchemaError::InvalidVersion);
        }

        validate_group_types(&schema.fields)?;
        Ok(schema)
    }

    pub fn load_from_file(path: &Path) -> Result<Self, SchemaError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_yaml(&content)
    }
}

fn validate_group_types(fields: &HashMap<String, FieldDefinition>) -> Result<(), SchemaError> {
    for (name, field) in fields {
        if let FieldDefinition::Group(group) = field {
            if group.group_type != "group" {
                return Err(SchemaError::InvalidGroupType(format!(
                    "{} (in field '{}')",
                    group.group_type, name
                )));
            }
            validate_group_types(&group.fields)?;
        }
    }
    Ok(())
}
