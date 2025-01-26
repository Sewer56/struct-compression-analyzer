# Bit Distribution Schema Documentation

This document describes the YAML schema format used for analyzing bit-packed structures.

## Schema Overview

The schema is designed to represent and analyze bit-packed structures with the following capabilities:

- Define individual fields with precise bit positions
- Group related fields together
- Create nested field hierarchies
- Support for different field types and metadata
- Group analysis results by field values

## Schema Structure

### Version Field

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

### Fields Section

The `fields` section defines all the fields and groups in the structure.  
Each field can be either a basic field or a group.  

The data is read as big endian, so bits `0-4` always appear in the first byte; `8-12` appear in the
second byte, etc.

#### Basic Fields

```yaml
field_name: 
  type: field
  bits: [start, end] # Inclusive bit range
  description: text  # Optional field description
  bit_order: order   # Optional, either "msb" (default) or "lsb"
                     # msb means 001 == 1
                     # lsb means 001 == 8
```

Basic fields represent individual components of the structure. The `bits` property uses an
inclusive range where both start and end bits are part of the field.

#### Groups

```yaml
group_name:
  type: group
  bits: [start, end]  # Total range for all components
  description: text   # Optional group description
  fields:             # For nested groups
    subfield1: ...
    subfield2: ...
  components:         # For flat groups
    comp1: [start, end]
    comp2: [start, end]
```

Groups can be structured in two ways:

1. Nested groups using the `fields` property
2. Flat groups using the `components` property

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
  bits: [0, 0]
  description: Mode bit
```

### Multi-bit Field

```yaml
partition:
  bits: [1, 4]
  description: Partition value
```

### Nested Group Structure

```yaml
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
        # ...
```

### Flat Group Structure

```yaml
p_bits:
  type: group
  bits: [77, 82]
  description: P-bits flags
  components:
    P0: [77, 77]
    P1: [78, 78]
    # ...
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

4. Analysis Configuration
   - Group by fields that have meaningful distributions
   - Provide clear labels for group values when applicable
   - Use hierarchical field names (e.g., "colors.r.R0") to access nested fields

## Complete Example

```yaml
version: '1.0'
metadata:
  name: BC1 Mode0 Block
  description: Analysis schema for Mode0 packed color structure with mode, partition, and color components

analysis:
  group_by:
    - field: partition
      description: Results grouped by partition value
      display:
        format: "Partition %d"

fields:
  # Basic fields
  mode:
    type: field
    bits: [0, 0]  # Single bit field
    description: Mode bit
    bit_order: msb  # Default ordering (could be omitted)

  partition:
    type: field
    bits: [1, 4]  # 4-bit field
    description: Partition value
    bit_order: lsb  # Interpret bits in reverse order

  # Color component groups
  colors:
    type: group
    description: All color components
    fields:
      r:
        type: group
        bits: [5, 28]  # Full range for all R components
        components:
          R0: [5, 8]
          R1: [9, 12]
          R2: [13, 16]
          R3: [17, 20]
          R4: [21, 24]
          R5: [25, 28]
      
      g:
        type: group
        bits: [29, 52]  # Full range for all G components
        components:
          G0: [29, 32]
          G1: [33, 36]
          G2: [37, 40]
          G3: [41, 44]
          G4: [45, 48]
          G5: [49, 52]
      
      b:
        type: group
        bits: [53, 76]  # Full range for all B components
        components:
          B0: [53, 56]
          B1: [57, 60]
          B2: [61, 64]
          B3: [65, 68]
          B4: [69, 72]
          B5: [73, 76]

  # P-bits group
  p_bits:
    type: group
    bits: [77, 82]
    description: P-bits flags
    components:
      P0: [77, 77]
      P1: [78, 78]
      P2: [79, 79]
      P3: [80, 80]
      P4: [81, 81]
      P5: [82, 82]

  # Index field (subdivided into groups of 3 bits each)
  index:
    type: group
    bits: [83, 127]
    description: 45-bit index field
    components:
      index0: [83, 85]
      index1: [86, 88]
      index2: [89, 91]
      index3: [92, 94]
      index4: [95, 97]
      index5: [98, 100]
      index6: [101, 103]
      index7: [104, 106]
      index8: [107, 109]
      index9: [110, 112]
      index10: [113, 115]
      index11: [116, 118]
      index12: [119, 121]
      index13: [122, 124]
      index14: [125, 127]
```
