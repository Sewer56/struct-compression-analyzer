# Bit Distribution Schema Documentation

This document describes the YAML schema format used for analyzing bit-packed structures.

## Schema Overview

The schema is designed to represent and analyze bit-packed structures with the following capabilities:

- Define individual fields with precise bit lengths
- Group related fields together
- Create nested field hierarchies
- Support for different field types and metadata
- Group analysis results by field values

## Schema Structure

### Top-Level Fields

```yaml
version: '1.0'
metadata: ...
conditional_offsets: ..
analysis: ...
root: ....
```

#### Version Field

```yaml
version: '1.0'
```

Specifies the schema format version. This allows for future schema evolution while maintaining backwards compatibility.

### Metadata

```yaml
metadata:
  name: structure_name
  description: Description of the structure
```

Contains high-level information about the structure being analyzed.

### Analysis Configuration

```yaml
analysis:
  split_groups:
    - name: split_colors
      group_1: [colors]          # Base group to compare against.
      group_2: [color0, color1]  # Derived group to compare with.
      description: Compare regular interleaved colour format `colors` against their split components `color0` and `color1`.
  compare_groups:
    - name: interleave_colours
      group_1: # Base group to compare against.
      group_2: # Derived group to compare with.
      description: Interleave colours such that `R0 R1 G0 G1 B0 B1` are now `R0 G0 B0 R1 G1 B1`.
```

The `analysis` section configures how results should be analyzed and presented:

- `split_groups`: Compare original field vs its split components
  - This feature allows you to compare a field (or group of fields) against each other.
  - A common use case is to compare a struct, or sub struct against its inner components.
    - This allows you to compare `structure of array` vs `array of structure` very easily.
  - `group_1` is used as baseline, while `group_2` is compared against it.
- `compare_groups`: Compare custom groups of fields against each other.
  - This allows you to define two structures based on existing fields in the file, and compare them.
  - Read [Custom Compare Groups](#custom-compare-groups) for more information.

### Conditional Offsets

```yaml
conditional_offsets:
  # BC7 format detection
  - offset: 0x94  # BC7 data starts at 148 bytes
    conditions:
      - byte_offset: 0x00 # file magic
        bit_offset: 0
        bits: 32
        bit_order: msb
        value: 0x44445320 # DDS magic
      - byte_offset: 0x54 # ddspf.dourCC
        bit_offset: 0
        bits: 32
        bit_order: msb
        value: 0x44583130 # DX10 header
      - byte_offset: 0x80 # ds_header_dxt10.dxgiFormat
        bit_offset: 0
        bits: 32
        bit_order: msb
        value: 0x62000000 # DXGI_FORMAT_BC7_UNORM
```

Conditional offsets validate headers in specified order using big-endian (by default) comparisons:

1. First checks for DDS magic number `0x44445320` (DDS) at offset 0
2. Then verifies DX10 header `0x44583130` (DX10) at offset 0x54
3. Finally confirms BC7 format `0x62000000` (BC7) at offset 0x80
4. If all three match, sets offset to 148 bytes (0x94)

The hex values are specified in big-endian byte order; i.e. the same order as you would
see in a hex editor. This can however be overwritten using the `bit_order` field; same
way you can with regular fields.

### Root Section

The `root` section defines the top-level structure containing all fields and groups.
Fields are written sequentially to the file, with offsets determined by the order and size of preceding fields.

#### Basic Fields

Fields can be defined in two ways:

1. **Shorthand notation** - Direct bit count specification:
```yaml
field_name: 3  # Field using 3 bits
```

2. **Extended notation** - Full field configuration:
```yaml
field_name: 
  type: field
  bits: 3            # Number of bits for the field. Auto calculated from children if not set.
  description: text  # Optional field description
  bit_order: order   # Optional, either "msb" (default) or "lsb"
  skip_frequency_analysis: true  # Optional, skips value frequency counting.
  skip_if_not:       # Optional list of validation conditions. This field is skipped if any condition fails
                     # See 'Conditional Offsets' for details on the syntax.
    - byte_offset: 0x00  # File offset to check
      bit_offset: 0      # Bit offset within byte (0-7)
      bits: 32           # Number of bits to compare (1-64)
      value: 0x44445320  # Expected big-endian value
```

- Shorthand syntax is equivalent to a basic field with default values
- Extended syntax allows for additional metadata

Warning: It is assumed each field has a unique name; this includes subfields.

It is recommended to use `skip_frequency_analysis` for any large fields (>24 bits) that are hugely random
while scanning large amounts of data; otherwise you'll experience significant performance losses.

#### Groups

```yaml
group_name:
  type: group
  description: text   # Optional group description
  bit_order: order    # Optional, either "msb" (default) or "lsb"
                      # If set here, all contained fields will inherit this order.
                      # Unless explicitly overwritten
  skip_frequency_analysis: true  # Optional, skips value frequency counting.
  skip_if_not:        # Optional list of validation conditions. This group is skipped if any condition fails
                      # See 'Conditional Offsets' for details on the syntax.
    - byte_offset: 0x00  # File offset to check
      bit_offset: 0      # Bit offset within byte (0-7)
      bits: 32           # Number of bits to compare (1-64)
      value: 0x44445320  # Expected big-endian value
  fields:             # Contained fields and sub-groups
    subfield1: 3      # 3-bit field
    subfield2: 4      # 4-bit field
```

Groups contain a collection of fields that are written sequentially:
- Nested groups
- Basic fields
- Mixed hierarchies of fields and groups

#### Endianness

To avoid confusion, endianness is specified in the following way:

```yaml
bit_order: msb  # Default, values are interpreted with bits left-to-right
bit_order: lsb  # Values are interpreted with bits right-to-left
```

To illustrate, consider the bits `10000000`; if we read the first 2 bits:

- `msb`: `10` equals `2` (decimal)
- `lsb`: `10` equals `1` (decimal)

MSB vs LSB does not change from which end of the byte we start reading bits from, but the order
of the bits of the individual values we extract. The order of bits read is always highest to lowest.

## Example Usage

Here's how different types of fields and analysis configurations are represented:

### Analysis Configuration Example

```yaml
analysis:
  split_groups:
    - name: split_colors
      group_1: [colors]          # Base group to compare against.
      group_2: [color0, color1]  # Derived group to compare with.
      description: Compare regular interleaved colour format `colors` against their split components `color0` and `color1`.
```

### Single Bit Field

```yaml
mode:
  type: field
  bits: 1
  description: Mode bit
```

### Multi-bit Field

```yaml
partition: 4  # 4-bit field
```

### Nested Group Structure

```yaml
colors:
  type: group
  description: All color components
  fields:
    r:
      type: group
      fields:
        R0: 4     # 4-bit field
        R1: 4     # 4-bit field
```

### Flat Group Structure

```yaml
p_bits:
  type: group
  description: P-bits flags
  fields:
    P0: 1    # Single-bit field
    P1: 1
    P2: 1
```

### Custom Compare Groups

The `compare_groups` section allows you to define custom field groups for comparison. 

Each group entry allows for one of the following:

- **Array**: Reads all values of a single field until end of input.
  - i.e. `R0`, `R0`, `R0` etc.. until all R0 values are read.
  - This is read in a loop until no more bytes are written to output.

- **Struct**: Groups multiple fields sequentially as one. Used to create interleaved elements.
  - **Field**: Includes a single field/value from the input.
  - **Padding**: Inserts constant bits to enable alignment or size adjustments in struct.
  - This is read in a loop until no more (non-padding) bytes are written to output.

The groups read the input data in a 'streaming'-like fashion.

For instance, every time a field `R` is included in a struct, then the next item from the `R` field
will be read. If there is no more next item for the given field, nothing will be appended.

#### Example 1: Interleaving Colours with Mixed Representations

```yaml
compare_groups:
  - name: interleave_colours
    description: "Rearrange interleaved colour channels from [R0 R1] [G0 G1] [B0 B1] to [R0 G0 B0] [R1 G1 B1]."
    group_1: # RRR GGG BBB etc.
      - { type: array, field: R } # reads all 'R' values from input
      - { type: array, field: G } # reads all 'G' values from input
      - { type: array, field: B } # reads all 'B' values from input
    group_2:
      - type: struct # R0 G0 B0. Repeats until no data written.
        fields:
          - { type: field, field: R } # reads 1 'R' value from input
          - { type: field, field: G } # reads 1 'G' value from input
          - { type: field, field: B } # reads 1 'B' value from input
```

In this case, interleaving usually improves ratio.

#### Example 2: Converting 7-bit to 8-bit Colours with Padding

Convert a 7-bit color value to an 8-bit representation by adding a padding bit.

```yaml
compare_groups:
  - name: convert_7_to_8_bit
    description: "Adjust 7-bit color channel to 8-bit by appending a padding bit."
    group_1: # R, R, R
      - { type: array, field: color7 } # reads all '7-bit' colours from input
    group_2:
      - type: struct # R+0, R+0, R+0
        fields:
          - { type: field, field: color7 } # reads 1 '7-bit' colour from input
          - { type: padding, bits: 1, value: 0 } # appends 1 padding bit
```

In this case, extending to 8 bits usually improves ratio.

## Best Practices

1. Group Related Fields
   - Use groups to organize related fields (like color components)
   - Nest groups when there's a clear hierarchy

2. Consistent Naming
   - Use descriptive names for fields and groups
   - Follow a consistent naming convention within each structure

3. Documentation
   - Include descriptions for complex fields and groups
   - Document any special cases or requirements

4. Group Composition
   - Use `fields` for both hierarchical and flat group structures
   - Prefer nested groups over flat structures when logical hierarchy exists
   - Maintain consistent field definition styles within a group

## Complete Example

For examples of the schema, check out the [schemas](./schemas) directory.  
That directory includes schemas for various formats I've worked with.  

If you're making a new schema, consider shooting a PR, I'd be happy to link to more examples.  