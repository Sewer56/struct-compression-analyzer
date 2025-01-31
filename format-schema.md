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
  group_by:
    - field: field_name # Name of the field to group by
      description: text # Optional description of this grouping
      display:          # Optional display configuration
        format: text    # How to format the group value
        labels:         # Optional value labels
          0: "Label for value 0"
          1: "Label for value 1"
    - field: another_field
      description: text
```

The `analysis` section configures how results should be analyzed and presented:

- `group_by`: List of fields to use for grouping results
  - Each entry specifies a field and optional display configuration
  - Multiple fields allow for different views of the same data
  - The `display` section can customize how groups are presented:
    - `format`: Printf-style format string (e.g., "%d", "%02x", "%s")
    - Common format specifiers:
      - `%d` - decimal integer
      - `%x` or `%X` - hexadecimal (lowercase/uppercase)
      - `%02d` - zero-padded decimal
      - `%s` - string
  - `labels` can provide meaningful names for specific values

### Conditional Offsets

```yaml
conditional_offsets:
  # BC7 format detection
  - offset: 0x94  # BC7 data starts at 148 bytes
    conditions:
      - byte_offset: 0x00 # file magic
        bit_offset: 0
        bits: 32
        value: 0x44445320 # DDS magic
      - byte_offset: 0x54 # ddspf.dourCC
        bit_offset: 0
        bits: 32
        value: 0x44583130 # DX10 header
      - byte_offset: 0x80 # ds_header_dxt10.dxgiFormat
        bit_offset: 0
        bits: 32
        value: 0x62000000 # DXGI_FORMAT_BC7_UNORM
```

Conditional offsets validate headers in specified order using big-endian comparisons:

1. First checks for DDS magic number `0x44445320` (DDS) at offset 0
2. Then verifies DX10 header `0x44583130` (DX10) at offset 0x54
3. Finally confirms BC7 format `0x62000000` (BC7) at offset 0x80
4. If all three match, sets offset to 148 bytes (0x94)

The hex values are specified in big-endian byte order; i.e. the same order as you would
see in a hex editor.

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
  fields:             # Contained fields and sub-groups
    subfield1: 3      # 3-bit field
    subfield2: 4      # 4-bit field
```

Groups contain a collection of fields that are written sequentially:
- Nested groups
- Basic fields
- Mixed hierarchies of fields and groups

## Example Usage

Here's how different types of fields and analysis configurations are represented:

### Analysis Configuration Example

```yaml
analysis:
  group_by:
    - field: partition
      description: Results grouped by partition value
      display:
        format: "Partition %d"
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

```yaml
version: '1.0'
metadata:
  name: BC1 Mode0 Block
  description: Analysis schema for Mode0 packed color structure with mode, partition, and color components

conditional_offsets:
  # BC7 format detection
  - offset: 0x94  # BC7 data starts at 148 bytes
    conditions:
      - byte_offset: 0x00 # file magic
        bit_offset: 0
        bits: 32
        value: 0x44445320 # DDS magic
      - byte_offset: 0x54 # ddspf.dourCC
        bit_offset: 0
        bits: 32
        value: 0x44583130 # DX10 header
      - byte_offset: 0x80 # ds_header_dxt10.dxgiFormat
        bit_offset: 0
        bits: 32
        value: 0x62000000 # DXGI_FORMAT_BC7_UNORM

analysis:
  group_by:
    - field: partition
      description: Results grouped by partition value
      display:
        format: "Partition %d"

root:
  type: group
  fields:
    mode: 1       # 1-bit mode field
    partition: 4  # 4-bit partition field
    
    colors:
      type: group
      description: All color components
      fields:
        r:
          type: group
          fields:
            R0: 4
            R1: 4
            R2: 4
            R3: 4
            R4: 4
            R5: 4
        
        g:
          type: group
          fields:
            G0: 4
            G1: 4
            G2: 4
            G3: 4
            G4: 4
            G5: 4
        
        b:
          type: group
          fields:
            B0: 4
            B1: 4
            B2: 4
            B3: 4
            B4: 4
            B5: 4

    p_bits:
      type: group
      description: P-bits flags
      fields:
        P0: 1
        P1: 1
        P2: 1
        P3: 1
        P4: 1
        P5: 1

    index:
      type: group
      description: 45-bit index field divided into 3-bit segments
      fields:
        index0: 3
        index1: 3
        index2: 3
        index3: 3
        index4: 3
        index5: 3
        index6: 3
        index7: 3
        index8: 3
        index9: 3
        index10: 3
        index11: 3
        index12: 3
        index13: 3
        index14: 3
```