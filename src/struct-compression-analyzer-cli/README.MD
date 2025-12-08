# struct-compression-analyzer-cli

Command-line interface for analyzing and comparing lossless transforms of bit-packed binary structures.

[![Crates.io](https://img.shields.io/crates/v/struct-compression-analyzer-cli.svg)](https://crates.io/crates/struct-compression-analyzer-cli)

## Installation

```bash
cargo install struct-compression-analyzer-cli
```

## Usage

The CLI provides commands for analyzing individual files or entire directories of binary data.

### Analyze a Single File

```bash
struct-compression-analyzer-cli analyze-file --schema schemas/dxt1-block.yaml input.file
```

### Analyze a Directory

```bash
struct-compression-analyzer-cli analyze-directory --schema schemas/dxt1-block.yaml path/to/files/
```

### Generate Reports

Use the `--output` flag to generate detailed reports (CSV, plots):

```bash
struct-compression-analyzer-cli analyze-directory --schema schemas/dxt1-block.yaml path/to/files/ --output reports/
```

### Output Formats

The CLI supports different output formats:

- `concise` (default): Brief summary of results
- `detailed`: Full analysis with all metrics
- `csv`: CSV files for further analysis
- `plot`: Visualizations of the analysis

Specify format with the `-f` or `--format` flag:

```bash
struct-compression-analyzer-cli analyze-directory --schema schemas/dxt1-block.yaml path/to/files/ -f detailed
```

## Building from Source

If you want to build from source instead of installing from crates.io:

```bash
git clone https://github.com/Sewer56/struct-compression-analyzer.git
cd struct-compression-analyzer
cargo build --release
./target/release/struct-compression-analyzer-cli --help
```

Or run directly with cargo:

```bash
cargo run --release -- analyze-file --schema schemas/dxt1-block.yaml input.file
```

## Schema Files

The CLI requires a YAML schema file that defines the structure of your binary data. 

See the [schema documentation] for details on creating schema files, and check the [schemas directory] for examples.

## Memory Usage

The analyzer can use significant memory (up to ~2.5x the input data size). For large datasets on Linux, you may want to:

```bash
sudo sysctl vm.overcommit_memory=1
```

And create a large swapfile. See the [main documentation] for more details.

## Getting Help

For detailed documentation about the library and analysis concepts, see:

- [Main Documentation](../struct-compression-analyzer/README.MD)
- [Schema Format Guide](../../format-schema.md)
- [Repository](https://github.com/Sewer56/struct-compression-analyzer)

[schema documentation]: https://github.com/Sewer56/struct-compression-analyzer/blob/main/format-schema.md
[schemas directory]: https://github.com/Sewer56/struct-compression-analyzer/tree/main/schemas
[main documentation]: https://github.com/Sewer56/struct-compression-analyzer/blob/main/src/struct-compression-analyzer/README.MD
