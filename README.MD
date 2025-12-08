# struct-compression-analyzer

[![Crates.io](https://img.shields.io/crates/v/struct-compression-analyzer.svg)](https://crates.io/crates/struct-compression-analyzer)
[![Docs.rs](https://docs.rs/struct-compression-analyzer/badge.svg)](https://docs.rs/struct-compression-analyzer)
[![CI](https://github.com/Sewer56/struct-compression-analyzer/actions/workflows/rust.yml/badge.svg)](https://github.com/Sewer56/struct-compression-analyzer/actions)

A tool for analyzing and comparing lossless transforms of bit-packed binary structures.

## Quick Start

Install the CLI tool:

```bash
cargo install struct-compression-analyzer-cli
```

Analyze a file with a schema:

```bash
struct-compression-analyzer-cli analyze-file --schema schemas/dxt1-block.yaml input.file
```

Analyze a directory:

```bash
struct-compression-analyzer-cli analyze-directory --schema schemas/dxt1-block.yaml path/to/files/
```

See the [struct-compression-analyzer crate documentation](./src/struct-compression-analyzer/README.MD) for detailed usage and examples.

## Crates

- [struct-compression-analyzer](./src/struct-compression-analyzer/README.MD): Core library for analyzing bit-packed structures
- [struct-compression-analyzer-cli](./src/struct-compression-analyzer-cli/README.MD): Command-line interface

## Developer Manual

For step-by-step development guidance, see the [Developer Manual](https://reloaded-project.github.io/reloaded-templates-rust/manual/).

## Contributing

We welcome contributions! See the [Contributing Guide](https://reloaded-project.github.io/reloaded-templates-rust/manual/#contributing) for details.

## License

Licensed under [GPLv3](./LICENSE).
