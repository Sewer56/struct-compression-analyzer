use crate::schema::{Condition, ConditionalOffset};
use bitstream_io::{BigEndian, BitRead, BitReader};
use std::{
    fs::File,
    io::{self, Cursor, Read, Seek, SeekFrom},
};

pub fn try_evaluate_file_offset(
    conditional_offsets: &[ConditionalOffset],
    file: &mut File,
) -> io::Result<Option<u64>> {
    // Calculate maximum needed read length from all conditions
    let max_read = conditional_offsets
        .iter()
        .flat_map(|o| &o.conditions)
        .map(|c| c.byte_offset + (c.bits as u64).div_ceil(8)) // Bytes needed
        .max()
        .unwrap_or(0);

    // Read required portion without reopening file
    file.seek(SeekFrom::Start(0))?;
    let mut data = unsafe { Box::new_uninit_slice(max_read as usize).assume_init() };
    file.read_exact(&mut data)?;

    Ok(try_evaluate_offset(conditional_offsets, &data))
}

pub fn try_evaluate_offset(conditional_offsets: &[ConditionalOffset], data: &[u8]) -> Option<u64> {
    for offset_def in conditional_offsets {
        if matches_all_conditions(offset_def, data) {
            return Some(offset_def.offset);
        }
    }
    None
}

fn matches_all_conditions(offset_def: &ConditionalOffset, data: &[u8]) -> bool {
    offset_def
        .conditions
        .iter()
        .all(|cond| check_condition(cond, data))
}

fn check_condition(condition: &Condition, data: &[u8]) -> bool {
    let mut reader = BitReader::endian(Cursor::new(data), BigEndian);
    let start_bit = (condition.byte_offset * 8) + condition.bit_offset as u64;

    if reader.seek_bits(SeekFrom::Start(start_bit)).is_err() {
        return false;
    }

    match reader.read::<u64>(condition.bits as u32) {
        Ok(extracted) => extracted == condition.value,
        Err(_) => false,
    }
}

#[cfg(test)]
mod byte_tests {
    use super::*;
    use crate::schema::{Condition, ConditionalOffset};

    fn create_bc7_conditions() -> Vec<ConditionalOffset> {
        vec![ConditionalOffset {
            offset: 0x94,
            conditions: vec![
                Condition {
                    byte_offset: 0x00,
                    bit_offset: 0,
                    bits: 32,
                    value: 0x44445320,
                },
                Condition {
                    byte_offset: 0x54,
                    bit_offset: 0,
                    bits: 32,
                    value: 0x44583130,
                },
            ],
        }]
    }

    #[test]
    fn matches_valid_bc7_header() {
        let mut data = vec![0u8; 0x80 + 4];
        // Set DDS magic
        data[0x00..0x04].copy_from_slice(&[0x44, 0x44, 0x53, 0x20]);
        // Set DX10 header
        data[0x54..0x58].copy_from_slice(&[0x44, 0x58, 0x31, 0x30]);

        let conditions = create_bc7_conditions();
        assert_eq!(try_evaluate_offset(&conditions, &data), Some(0x94));
    }

    #[test]
    fn rejects_invalid_dx10_header() {
        let mut data = vec![0u8; 0x80 + 4];
        data[0x00..0x04].copy_from_slice(&[0x44, 0x44, 0x53, 0x20]);
        // Invalid DX10
        data[0x54..0x58].copy_from_slice(&[0x41, 0x42, 0x43, 0x44]);

        let conditions = create_bc7_conditions();
        assert_eq!(try_evaluate_offset(&conditions, &data), None);
    }

    #[test]
    fn handles_short_data() {
        let data = vec![0u8; 0x50]; // Too short for DX10 check

        let conditions = create_bc7_conditions();
        assert_eq!(try_evaluate_offset(&conditions, &data), None);
    }

    #[test]
    fn matches_valid_bc7_header_from_yaml() {
        let yaml_data = r#"
            - offset: 0x94
              conditions:
                - byte_offset: 0
                  bit_offset: 0
                  bits: 32
                  value: 0x44445320
                - byte_offset: 0x54
                  bit_offset: 0
                  bits: 32
                  value: 0x44583130
        "#;

        // This test ensures that the YAML parser works as expected,
        // with our value being treated in big endian form when specified as hex.
        let conditions: Vec<ConditionalOffset> = serde_yaml::from_str(yaml_data).unwrap();
        let mut data = vec![0u8; 0x80 + 4];
        // Set DDS magic
        data[0x00..0x04].copy_from_slice(&[0x44, 0x44, 0x53, 0x20]);
        // Set DX10 header
        data[0x54..0x58].copy_from_slice(&[0x44, 0x58, 0x31, 0x30]);
        assert_eq!(try_evaluate_offset(&conditions, &data), Some(0x94));
    }
}

#[cfg(test)]
mod bit_tests {
    use super::*;
    use crate::schema::{Condition, ConditionalOffset};

    // New bit-oriented tests will go here

    #[test]
    fn validates_bitstream_header() {
        let conditions = [ConditionalOffset {
            offset: 0,
            conditions: vec![
                Condition {
                    byte_offset: 0,
                    bit_offset: 4,
                    bits: 4,
                    value: 0b1110,
                },
                Condition {
                    byte_offset: 1,
                    bit_offset: 0,
                    bits: 8,
                    value: 0xC0,
                },
            ],
        }];

        // Valid header: 0xXXAXXC0XX (bits 4-7 = 0xA, byte 1 = 0xC0)
        let valid_data = [0x0E, 0xC0, 0x00];
        assert!(matches_all_conditions(&conditions[0], &valid_data));

        // Invalid header: bits 4-7 = 0xB
        let invalid_data = [0x0B, 0xC0, 0x00];
        assert!(!matches_all_conditions(&conditions[0], &invalid_data));
    }
}
