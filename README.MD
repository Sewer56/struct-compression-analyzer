# Lossless Transform Analysis Tool

A tool for analyzing and comparing lossless transforms of bit-packed binary structures.

[![Documentation](https://docs.rs/struct-compression-analyzer/badge.svg)][docs]
[![Tests](https://github.com/Sewer56/struct-compression-analyzer/workflows/rust/badge.svg)][actions]
[![Crates.io](https://img.shields.io/crates/v/struct-compression-analyzer.svg)][crates-badge]

## About

This crate provides functionality for analyzing and comparing lossless transforms of bit-packed
binary structures. 

The core idea being, that you can rearrange the order of bytes within a file, or modify the bytes
in a reversible way that improves compression ratio.

For some more context, see the '[Texture Compression in Nx2.0]' series.

## What this Program Does

You can:

- Define structures using a YAML schema
- Compare different data layouts for compression efficiency
  - e.g. `Array of Structure` vs. `Structure of Arrays`
- Generate detailed statistics about bit distribution, entropy, and compression ratios

And this is useful for optimizing:

- Huge files where disk space is precious.
  - `Textures`, `3D Models`, `Audio Data`
- Small data where bandwidth is limited.
  - e.g. Network packets in 64-player game.

## Features

- **Schema-based analysis**: Define your binary structure using a YAML schema
- **Field comparisons**: Compare different field layouts to find the most efficient packing
- **Entropy analysis**: Calculate Shannon entropy of fields and structures
- **Compression analysis**: Measure LZ77 matches and zstd compression ratios
- **CSV output**: Generate CSV reports for detailed analysis
- **Multi-threaded processing**: Efficiently process large directories of files
- **Custom group comparisons**: Define and analyze custom field groupings
- **Plot generation**: Visualize analysis results
- **Bit distribution analysis**: Examine individual bit patterns and frequencies

## Quick Start

If you have not already, install [Rust].
This should give you access to `cargo` in your terminal.

1. **Install the tool**:
   ```bash
   cargo install struct-compression-analyzer-cli
   ```

2. **Create a schema**:

   Define your binary structure in a YAML file. 
   For example, see the schemas in the [schemas directory].

   Refer to the [schema documentation] for more details.

3. **Analyze a file**:
   ```bash
   struct-compression-analyzer-cli analyze-file --schema schemas/dxt1-block.yaml input.file
   ```

4. **Analyze a directory**:
   ```bash
   struct-compression-analyzer-cli analyze-directory --schema schemas/dxt1-block.yaml path/to/files/
   ```

5. **Generate reports**:

   Use the `--output` flag to generate reports (CSV, Plot):

   ```bash
   struct-compression-analyzer-cli analyze-directory --schema schemas/dxt1-block.yaml path/to/files/ --output reports/
   ```

If you want to build from source, replace `struct-compression-analyzer-cli` with `cargo run --release --`.

## Example Schema

The schema documentation can be found in [format-schema.md], which explains how to
define your binary structures using YAML.

A trivial example however is provided below:

```yaml
version: '1.0'
metadata:
  name: DXT1/BC1 Block
  description: Analysis schema for DXT1/BC1 compressed texture block format
conditional_offsets: 
    # Add support for '.dds' container.
  - offset: 0x80  # DXT1 data starts at 128 bytes
    conditions:
      - byte_offset: 0x00  # file magic
        bit_offset: 0
        bits: 32
        value: 0x44445320  # DDS magic

      - byte_offset: 0x54  # fourCC field position
        bit_offset: 0
        bits: 32
        value: 0x44585431  # 'DXT1' fourCC code
root:
  type: group
  fields:
    colors:
      type: group
      fields:
        # Group/full notation
        color0:   
          type: group
          description: First RGB565 color value
          fields:
            r0: 5  # Red component
            g0: 6  # Green component
            b0: 5  # Blue component
        
        color1:
          type: group
          description: Second RGB565 color value
          fields:
            r1: 5  # Red component
            g1: 6  # Green component
            b1: 5  # Blue component

    # Shorthand notation
    indices: 32 # 32-bit indices for each texel (4x4 block = 16 texels)
```

## Example Usage

Command: 

```bash
cargo run --release analyze-directory --schema schemas/dxt1-block.yaml "202x-architecture-10.01" -f concise
```

Output:

```plaintext
Analyzing directory: 202x-architecture-10.01 (125 files)
Merging 125 files.

Aggregated (Merged) Analysis Results:
Schema: DXT1/BC1 Block
File: 6.78bpb, 9429814 LZ, 30165973/28322604/44739256 (67.43%/63.31%/100.00%) (est/zstd/orig)

colors: 5.50bpb, 9340190 LZ (99.05%), 4400265/3567275/10007659 (31.29%/26.58%/50.00%) (est/zstd/orig), 32bit
  color0: 5.42bpb, 4689715 LZ (50.21%), 2156012/1864829/5003829 (49.00%/52.28%/50.00%) (est/zstd/orig), 16bit
    r0: 6.97bpb, 483853 LZ (10.32%), 1223500/1276640/1563697 (56.75%/68.46%/31.25%) (est/zstd/orig), 5bit
    g0: 6.20bpb, 1333745 LZ (28.44%), 1063952/1088687/1876436 (49.35%/58.38%/37.50%) (est/zstd/orig), 6bit
    b0: 6.64bpb, 859181 LZ (18.32%), 1032181/1078998/1563697 (47.87%/57.86%/31.25%) (est/zstd/orig), 5bit
  color1: 4.96bpb, 4864159 LZ (52.08%), 1948434/1646466/5003829 (44.28%/46.15%/50.00%) (est/zstd/orig), 16bit
    r1: 6.47bpb, 685486 LZ (14.09%), 1072491/1174668/1563697 (55.04%/71.34%/31.25%) (est/zstd/orig), 5bit
    g1: 5.65bpb, 1538888 LZ (31.64%), 918205/950598/1876436 (47.13%/57.74%/37.50%) (est/zstd/orig), 6bit
    b1: 6.17bpb, 1042397 LZ (21.43%), 911990/976376/1563697 (46.81%/59.30%/31.25%) (est/zstd/orig), 5bit
indices: 6.66bpb, 2010712 LZ (21.32%), 7832755/8199754/10007659 (55.70%/61.10%/50.00%) (est/zstd/orig), 32bit
  index0: 7.59bpb, 15801 LZ (0.79%), 602831/601666/625478 (7.70%/7.34%/6.25%) (est/zstd/orig), 2bit
  index1: 7.47bpb, 18170 LZ (0.90%), 590045/588991/625478 (7.53%/7.18%/6.25%) (est/zstd/orig), 2bit
  index2: 7.46bpb, 18126 LZ (0.90%), 589961/588861/625478 (7.53%/7.18%/6.25%) (est/zstd/orig), 2bit
  index3: 7.59bpb, 15844 LZ (0.79%), 602763/601672/625478 (7.70%/7.34%/6.25%) (est/zstd/orig), 2bit
  index4: 7.49bpb, 15474 LZ (0.77%), 591857/589849/625478 (7.56%/7.19%/6.25%) (est/zstd/orig), 2bit
  index5: 7.25bpb, 24113 LZ (1.20%), 566404/566778/625478 (7.23%/6.91%/6.25%) (est/zstd/orig), 2bit
  index6: 7.24bpb, 24270 LZ (1.21%), 566036/566458/625478 (7.23%/6.91%/6.25%) (est/zstd/orig), 2bit
  index7: 7.49bpb, 15496 LZ (0.77%), 591746/589775/625478 (7.55%/7.19%/6.25%) (est/zstd/orig), 2bit
  index8: 7.49bpb, 15480 LZ (0.77%), 591819/589848/625478 (7.56%/7.19%/6.25%) (est/zstd/orig), 2bit
  index9: 7.25bpb, 24111 LZ (1.20%), 566285/566645/625478 (7.23%/6.91%/6.25%) (est/zstd/orig), 2bit
  index10: 7.25bpb, 24070 LZ (1.20%), 566446/566807/625478 (7.23%/6.91%/6.25%) (est/zstd/orig), 2bit
  index11: 7.49bpb, 15474 LZ (0.77%), 591852/589842/625478 (7.56%/7.19%/6.25%) (est/zstd/orig), 2bit
  index12: 7.59bpb, 16005 LZ (0.80%), 602431/601295/625478 (7.69%/7.33%/6.25%) (est/zstd/orig), 2bit
  index13: 7.46bpb, 18417 LZ (0.92%), 589630/588649/625478 (7.53%/7.18%/6.25%) (est/zstd/orig), 2bit
  index14: 7.46bpb, 18463 LZ (0.92%), 589616/588654/625478 (7.53%/7.18%/6.25%) (est/zstd/orig), 2bit
  index15: 7.59bpb, 16127 LZ (0.80%), 602316/601113/625478 (7.69%/7.33%/6.25%) (est/zstd/orig), 2bit

Split Group Comparisons:
  split_colors: Compare regular interleaved colour format `colors` against their split components `color0` and `color1`.
    Original Size: 10007659
    Base LZ, Entropy: (9340190, 5.50)
    Comp LZ, Entropy: (9560716, 5.50)
    Base Group LZ, Entropy: ([], [])
    Comp Group LZ, Entropy: ([], [])
    Base (est/zstd): (4400265/3567275)
    Comp (est/zstd): (4337413/3498364)
    Ratio (est/zstd): (98.57163148128578/98.06824536936458)
    Diff (est/zstd): (-62851/-68911)

Custom Group Comparisons:
  dxt1_transforms: Compare different arrangements of DXT1 block data
    Base Group:
      Size: 20015319
      LZ, Entropy: (9429814, 6.78)
      Est/Zstd: (14061599/13420865)

    colors_then_indices Group:
      Size: 20015319
      LZ, Entropy: (11350940, 6.78)
      Est/Zstd: (13437865/11767256)
      Ratio (est/zstd): (95.6%/87.7%)
      Diff (est/zstd): (-623734/-1653608)

    color0_color1_indices Group:
      Size: 20015319
      LZ, Entropy: (11571464, 6.78)
      Est/Zstd: (13364569/11698334)
      Ratio (est/zstd): (95.0%/87.2%)
      Diff (est/zstd): (-697030/-1722530)
```

Note the result above is an aggregate, i.e. the 'average' file.
See [dxt1-block.yaml] for reference schema.

What can we take away? Well, `colors` and `indices` have massive entropy difference.
A large difference in `entropy` or `lz matches` is a decent indicator that rearranging the data
would improve compression ratio.

We measure this with `colors_then_indices` custom comparison; which says that after the transform,
the average file is `88.3%` of the size, or in other words, `11.7%` smaller after transformation.

Recommended reading: '[Texture Compression in Nx2.0]' series.

## API

While this crate provides a Rust API, it is primarily designed to be used through its CLI interface.
However, the underlying functionality can be accessed programmatically:

```toml
# Cargo.toml
[dependencies]
struct-compression-analyzer = "0.1.0"
```

```rust no_run
use struct_compression_analyzer::results::PrintFormat;
use struct_compression_analyzer::schema::Schema;
use struct_compression_analyzer::analyzer::SchemaAnalyzer;
use struct_compression_analyzer::analyzer::CompressionOptions;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    // Load the schema
    let schema = Schema::load_from_file(Path::new("schema.yaml"))?;
    
    // Set the options
    let options = CompressionOptions::default();

    // Create an analyzer
    let mut analyzer = SchemaAnalyzer::new(&schema, options);
    
    // Add data to analyze
    analyzer.add_entry(&[0x01, 0x02, 0x03])?;
    
    // Generate results
    let results = analyzer.generate_results()?;
    
    // Print the results
    results.print(&schema, PrintFormat::Concise, false);
    
    Ok(())
}
```

For more detailed API documentation, see the [rustdocs](https://docs.rs/struct-compression-analyzer).  
Each module has its own documentation, so use the sidebar 😉.

No API stability is guaranteed whatsoever, this is a one-off three-weekend project; and is primarily
a CLI tool.

## Memory Usage Guideline

The program uses a lot of memory.  

### Expected Usage

***Expect up to ~2.5x the size of the input data in RAM usage for a 'typical' schema***.

Exact memory usage depends on complexity of the schema.
To improve memory usage and perf, I've replaced the standard memory allocator with [mi-malloc]; 
not something you do often, but it was useful here.

Peak memory usage is primarily correlated with:

#### The amount of nested fields/groups

This affects peak memory usage/spikes.

When you have a field `colours` with components `r`, `g`, `b`,
then you'll be storing the amount of data used by `colours` twice over, once for the
whole struct, and once for each component.

This is true all the way down, recursively.
More nesting means more memory usage.

#### Frequency Analysis on Fields

This is the main contributor to memory usage.

For performance and RAM reasons, the the max size of field for frequency analysis 
is capped at 16 bits. You may increase this via settings, if you want to analyze larger fields,
at expense of potentially *a lot* of memory.

If you extend to 32-bits for example, expect ~4x the memory usage of input data in terms of RAM.

If you want to analyze bigger fields, but want to avoid certain ones,
you can set `skip_frequency_analysis` to `true` on a field in schema to always skip that field.

### Running on Large Datasets on Linux

If you are running on Linux, you might want to:

```bash
sudo sysctl vm.overcommit_memory=1
```

And create a ***large swapfile*** on your system.

In my case, for 16GB of input, I used 32GiB of RAM + 32GiB of swap with a complex schema.

Running out of memory will get your process killed otherwise, and that's not easy to predict.
If you want to do a quick 'test run' first, set the `zstd compression level` to `1` in the CLI.

## Development

For information on how to work with this codebase, see [README-DEV.MD](README-DEV.MD).

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for details on how to contribute to this project.

[docs]: https://docs.rs/struct-compression-analyzer
[actions]: https://github.com/Sewer56/struct-compression-analyzer/actions
[crates-badge]: https://img.shields.io/crates/v/struct-compression-analyzer
[format-schema.md]: https://github.com/Sewer56/struct-compression-analyzer/blob/main/format-schema.md
[schema documentation]: https://github.com/Sewer56/struct-compression-analyzer/blob/main/format-schema.md
[Rust]: https://www.rust-lang.org/
[schemas directory]: https://github.com/Sewer56/struct-compression-analyzer/tree/main/schemas
[dxt1-block.yaml]: https://github.com/Sewer56/struct-compression-analyzer/tree/main/schemas/dxt1-block.yaml
[Texture Compression in Nx2.0]: https://sewer56.dev/blog/category/texture-compression-in-nx20.html
[mi-malloc]: https://github.com/microsoft/mimalloc