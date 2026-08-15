# Wizard

Wizard is a fast, fuzzy-aware alternative to `which`. It finds an exact executable in
`PATH`, or suggests the closest executable names when the command was misspelled.

This is the Rust successor to Warlock. It preserves Warlock's command-line options,
matching algorithms, executable filtering, `PATH` precedence, and colored suggestion
table while replacing the shell/BEAM launcher pair with one native binary.

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
- `--include-windows` includes Windows-mounted `PATH` directories in fuzzy searches on WSL.
- `--help` and `--version` print program information.

Wizard scans distinct `PATH` directories in parallel, considers only runnable files,
preserves the earliest executable for duplicate names, and ranks qualifying results by
score and then name. WSL-mounted Windows paths and native Windows `PATHEXT` entries receive
the same command-extension handling as Warlock. On WSL, mounted Windows `PATH` directories
are skipped during fuzzy searches because enumerating DrvFS directories is comparatively
slow. Exact Windows commands still resolve normally; use `--include-windows` when fuzzy
Windows-command suggestions are desired.

## Development

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
```

## Migration validation

The original Elixir project passed all 11 of its tests before migration. Wizard passes 17
Rust tests, including end-to-end exact and fuzzy CLI checks, and is clean under Clippy and
`cargo fmt --check`.

On 2026-08-15, a Hyperfine comparison used a local `PATH` containing 5,000 synthetic
executables plus `/usr/bin`, with 3 warmups and 20 measured fuzzy searches. Both versions
returned the same five suggestions in the same order.

| Implementation | Mean | Standard deviation |
| --- | ---: | ---: |
| Warlock (Elixir/BEAM) | 374.4 ms | 28.1 ms |
| Wizard (Rust release) | 17.7 ms | 1.3 ms |

Wizard was **21.10 ± 2.20 times faster** in that migration benchmark.

The WSL-aware behavior was also measured against the machine's real `PATH` (65 unique
directories, including 39 Windows-mounted directories), with 5 warmups and 15 runs:

| Implementation | Mean | Standard deviation |
| --- | ---: | ---: |
| Warlock | 528.9 ms | 26.8 ms |
| Wizard, WSL-aware default | 50.7 ms | 4.8 ms |
| Wizard with `--include-windows` | 448.3 ms | 20.4 ms |

The WSL-aware default was **10.44 ± 1.12 times faster than Warlock** on that `PATH`.

## License

[MIT](LICENSE)
