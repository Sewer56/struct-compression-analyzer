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
  compare_groups:
    - name: split_colors
      group_1: [colors]          # Base group to compare against.
      group_2: [color0, color1]  # Derived group to compare with.
      description: Compare regular interleaved colour format `colors` against their split components `color0` and `color1`.
```

The `analysis` section configures how results should be analyzed and presented:

- `compare_groups`: Custom group comparisons
  - This feature allows you to compare fields (or groups) against each other.
  - A common use case is to compare a struct, or sub struct against its inner components.
    - This allows you to compare `structure of array` vs `array of structure` very easily.
  - `group_1` is used as baseline, while `group_2` is compared against it.

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

#### Groups

```yaml
group_name:
  type: group
  description: text   # Optional group description
  bit_order: order    # Optional, either "msb" (default) or "lsb"
                      # If set here, all contained fields will inherit this order.
                      # Unless explicitly overwritten
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
  compare_groups:
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