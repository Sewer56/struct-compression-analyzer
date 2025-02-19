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

5. **Generate CSV reports**:

   Use the `--csv-output` flag to generate CSV reports:

   ```bash
   struct-compression-analyzer-cli analyze-directory --schema schemas/dxt1-block.yaml path/to/files/ --csv-output reports/
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

Field Metrics:
colors: 5.50bpb, 9340190 LZ (99.05%), 4400265/3567343/10007659 (14.59%/12.60%/22.37%) (est/zstd/orig), 32bit
  color0: 5.42bpb, 4689715 LZ (50.21%), 2156012/1864956/5003829 (49.00%/52.28%/50.00%) (est/zstd/orig), 16bit
    r0: 6.97bpb, 483853 LZ (10.32%), 1223500/1276639/1563697 (56.75%/68.45%/31.25%) (est/zstd/orig), 5bit
    g0: 6.20bpb, 1333745 LZ (28.44%), 1063952/1088734/1876436 (49.35%/58.38%/37.50%) (est/zstd/orig), 6bit
    b0: 6.64bpb, 859181 LZ (18.32%), 1032181/1079024/1563697 (47.87%/57.86%/31.25%) (est/zstd/orig), 5bit
  color1: 4.96bpb, 4864159 LZ (52.08%), 1948434/1646509/5003829 (44.28%/46.16%/50.00%) (est/zstd/orig), 16bit
    r1: 6.47bpb, 685486 LZ (14.09%), 1072491/1174673/1563697 (55.04%/71.34%/31.25%) (est/zstd/orig), 5bit
    g1: 5.65bpb, 1538888 LZ (31.64%), 918205/950646/1876436 (47.13%/57.74%/37.50%) (est/zstd/orig), 6bit
    b1: 6.17bpb, 1042397 LZ (21.43%), 911990/976409/1563697 (46.81%/59.30%/31.25%) (est/zstd/orig), 5bit
indices: 6.66bpb, 2010712 LZ (21.32%), 7832755/8199783/10007659 (25.97%/28.95%/22.37%) (est/zstd/orig), 32bit
  index0: 7.59bpb, 15801 LZ (0.79%), 602831/601666/625478 (7.70%/7.34%/6.25%) (est/zstd/orig), 2bit
  index1: 7.47bpb, 18170 LZ (0.90%), 590045/588991/625478 (7.53%/7.18%/6.25%) (est/zstd/orig), 2bit
  index2: 7.46bpb, 18126 LZ (0.90%), 589961/588861/625478 (7.53%/7.18%/6.25%) (est/zstd/orig), 2bit
  index3: 7.59bpb, 15844 LZ (0.79%), 602763/601672/625478 (7.70%/7.34%/6.25%) (est/zstd/orig), 2bit
  index4: 7.49bpb, 15474 LZ (0.77%), 591857/589849/625478 (7.56%/7.19%/6.25%) (est/zstd/orig), 2bit
  index5: 7.25bpb, 24113 LZ (1.20%), 566404/566778/625478 (7.23%/6.91%/6.25%) (est/zstd/orig), 2bit
  index6: 7.24bpb, 24270 LZ (1.21%), 566036/566458/625478 (7.23%/6.91%/6.25%) (est/zstd/orig), 2bit
  index7: 7.49bpb, 15496 LZ (0.77%), 591746/589775/625478 (7.55%/7.19%/6.25%) (est/zstd/orig), 2bit
  index8: 7.49bpb, 15480 LZ (0.77%), 591819/589847/625478 (7.56%/7.19%/6.25%) (est/zstd/orig), 2bit
  index9: 7.25bpb, 24111 LZ (1.20%), 566285/566646/625478 (7.23%/6.91%/6.25%) (est/zstd/orig), 2bit
  index10: 7.25bpb, 24070 LZ (1.20%), 566446/566806/625478 (7.23%/6.91%/6.25%) (est/zstd/orig), 2bit
  index11: 7.49bpb, 15474 LZ (0.77%), 591852/589840/625478 (7.56%/7.19%/6.25%) (est/zstd/orig), 2bit
  index12: 7.59bpb, 16005 LZ (0.80%), 602431/601292/625478 (7.69%/7.33%/6.25%) (est/zstd/orig), 2bit
  index13: 7.46bpb, 18417 LZ (0.92%), 589630/588648/625478 (7.53%/7.18%/6.25%) (est/zstd/orig), 2bit
  index14: 7.46bpb, 18463 LZ (0.92%), 589616/588653/625478 (7.53%/7.18%/6.25%) (est/zstd/orig), 2bit
  index15: 7.59bpb, 16127 LZ (0.80%), 602316/601114/625478 (7.69%/7.33%/6.25%) (est/zstd/orig), 2bit

Split Group Comparisons:
  split_colors: Compare regular interleaved colour format `colors` against their split components `color0` and `color1`.
    Original Size: 22369628
    Base LZ, Entropy: (22330102, 4.87)
    Comp LZ, Entropy: (22337863, 4.87)
    Ratio (est/zstd): (99.97921134917289/100.4934079707273)
    Diff (est/zstd): (-1773/28970)

Custom Group Comparisons:
  dxt1_transforms: Compare different arrangements of DXT1 block data
    Base Group:
      Size: 44739256
      LZ, Entropy: (23070341, 6.69)
      Est/Zstd: (30165973/28322604)
      Ratio (est/zstd): (67.43%/63.31%)
      Diff (est/zstd): (-817971/-3314946)
    colors_then_indices Group:
      Size: 44739256
      LZ, Entropy: (25679802, 6.69)
      Est/Zstd: (29348002/25007658)
      Ratio (est/zstd): (97.3%/88.3%)
      Diff (est/zstd): (-820406/-3286069)
    color0_color1_indices Group:
      Size: 44739256
      LZ, Entropy: (25687567, 6.69)
      Est/Zstd: (29345567/25036535)
      Ratio (est/zstd): (97.3%/88.4%)
      Diff (est/zstd): (-820406/-3286069)
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
use struct_compression_analyzer::analysis_results::PrintFormat;
use struct_compression_analyzer::schema::Schema;
use struct_compression_analyzer::analyzer::SchemaAnalyzer;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    // Load the schema
    let schema = Schema::load_from_file(Path::new("schema.yaml"))?;
    
    // Create an analyzer
    let mut analyzer = SchemaAnalyzer::new(&schema);
    
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

***Expect around 3.2-4x the size of the input data***; exact memory usage depends on complexity of the schema.

Memory usage is largely correlated on the amount of nested fields/groups, and whether you're doing
frequency analysis for a field or not. Setting `skip_frequency_analysis` to `true` in the schema
might help alleviate memory usage problems if that is a bottleneck.

If you are running on Linux, you might want to:

```bash
# on Linux
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