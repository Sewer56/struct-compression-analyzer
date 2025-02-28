# STRUCT-COMPRESSION-ANALYZER

## Build Commands
- `cargo build` - Build the project
- `cargo run --release` - Run in release mode
- `cargo test` - Run all tests
- `cargo test test_name` - Run a specific test
- `cargo clippy` - Run the linter

## Test & Coverage
- `cargo watch -x "test"` - Auto-test on save
- `cargo tarpaulin --out Xml --out Html --engine llvm --target-dir target/coverage-build` - Generate coverage

## Code Style Guidelines
- **Formatting**: Uses `cargo fmt` (VSCode formats on save)
- **Imports**: Standard Rust ordering by module/crate
- **Documentation**: Use module-level doc comments (//!) and function-level docs
- **Error Handling**: Use anyhow for errors with Result pattern
- **Naming**: Follow Rust conventions (snake_case for functions, CamelCase for types)
- **Commits**: Use "Keep a Changelog" format (Added/Changed/Deprecated/Removed/Fixed/Security)
- **Memory**: Expect ~2.5x the size of input data in RAM for typical usage

## Tools
- VS Code extensions: rust-analyzer, coverage-gutters, CodeLLDB, crates
- Clippy runs on save (configured in .vscode/settings.json)