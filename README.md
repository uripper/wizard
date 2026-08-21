# Wizard

Wizard is a fast, fuzzy-aware alternative to `which`. It finds an exact executable in
`PATH`, or suggests the closest executable names if the command was misspelled.


## Build and install

```bash
cargo build --release
cargo install --path .
```

The optimized binary is written to `target/release/wizard`.

## Usage

```text
wizard [options] command
```

```bash
wizard wimich
wizard wimich --verbose
wizard wimich --sensitivity=.5 --algorithm=lev --threshold=.7
```

Options:

- `--verbose` prints search details and similarity scores.
- `--sensitivity FLOAT` sets the weighted Levenshtein substitution cost (default `1.0`).
- `--algorithm jaro_winkler|jw|levenshtein|lev` selects the scorer (default Jaro-Winkler).
- `--threshold FLOAT` sets the minimum score from `0.0` to `1.0` (default `0.75`).
- `--matches INTEGER` limits displayed suggestions (default `5`).
- `--ignore LIST` ignores comma-separated filename fragments.
- `--ignoredir LIST` ignores comma-separated `PATH` directory fragments.
- `--include-windows` includes Windows-mounted `PATH` directories in fuzzy searches if on WSL.
- `--help` and `--version` print program information.


## Development

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
```

## License

[MIT](LICENSE)
