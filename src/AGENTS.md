# struct-compression-analyzer

A tool for analyzing and comparing lossless transforms of bit-packed binary structures.
Helps optimize compression by finding the best data arrangement for different formats.

# Project Structure

- `struct-compression-analyzer/` - Main library crate
  - `src/` - Library source code
- `struct-compression-analyzer-cli/` - CLI executable wrapper

# Code Guidelines

- Optimize for performance; use zero-cost abstractions, avoid allocations.
- Keep modules under 500 lines (excluding tests); split if larger.
- Place `use` inside functions only for `#[cfg]` conditional compilation.

# Documentation Standards

- Document public items with `///`
- Add examples in docs where helpful
- Use `//!` for module-level docs
- Focus comments on "why" not "what"
- Use [`TypeName`] rustdoc links, not backticks.

# Post-Change Verification

```bash
cargo test --workspace --all-features
cargo clippy --workspace --all-features -- -D warnings
cargo doc --workspace --all-features
cargo fmt --all
cargo publish --dry-run
```

All must pass before submitting.
