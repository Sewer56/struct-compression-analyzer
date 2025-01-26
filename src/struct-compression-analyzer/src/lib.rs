#![doc = include_str!("../../../README.MD")]

pub mod schema;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_schema() {
        let yaml = r#"
            version: '1.0'
            metadata:
              name: Test Schema
            fields:
              test_field:
                bits: [0, 4]
        "#;

        let schema = schema::Schema::from_yaml(yaml).unwrap();
        assert_eq!(schema.version, "1.0");
        assert_eq!(schema.metadata.name, "Test Schema");
    }
}
