use crate::{
    analyze_utils::create_bit_writer_with_owned_data, analyzer::AnalyzerFieldState,
    schema::BitOrder,
};
use ahash::{AHashMap, HashMapExt};
use rustc_hash::FxHashMap;

/// Name of the constant used for the test field
/// when a known name does not need to be specified.
pub(crate) const TEST_FIELD_NAME: &str = "test_field";

/// Creates a mock [`FieldStats`] instance for testing.
///
/// # Arguments
///
/// * `data` - The data to use for the mock field stats.
/// * `len_bits` - The length of a single field in bits.
/// * `data_bit_order` - The bit order of the data being written. (bit order of file)
/// * `field_bit_order` - The bit order of the field. (if first or last bit is stored first)
pub(crate) fn create_mock_field_states(
    field_name: &str,
    data: &[u8],
    len_bits: u32,
    data_bit_order: BitOrder,
    field_bit_order: BitOrder,
) -> AHashMap<String, AnalyzerFieldState> {
    let mut map = AHashMap::new();
    let writer = create_bit_writer_with_owned_data(data, data_bit_order);
    let name = field_name.to_string();

    map.insert(
        name.clone(),
        AnalyzerFieldState {
            name: name.clone(),
            full_path: name.clone(),
            bit_counts: Vec::new(),
            bit_order: field_bit_order,
            count: 0,
            depth: 0,
            value_counts: FxHashMap::new(),
            writer,
            lenbits: len_bits,
        },
    );
    map
}
