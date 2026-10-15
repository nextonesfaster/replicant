# Replicant

Replicant checks whether two regular expressions match exactly the same strings.
When they differ, it finds a shortest counterexample.

## Usage

```sh
$ vk 'ab|ac' 'a(b|c)'
equivalent

$ vk 'a(b|c)' 'a(b|c)*'
different
counterexample: "a"
matches: right
```

Patterns match the entire input. Counterexamples are shortest by UTF-8 byte
length and may be empty. Quotes and control characters are escaped in the output.
The supported syntax and resource limits are described in the
[design notes](docs/design.md).

Use `--` before patterns that begin with a hyphen:

```sh
vk -- '-a' '[-]a'
```

Run `vk --help` for help or `vk --version` for the version.

| Exit code | Meaning |
| --- | --- |
| `0` | The patterns are equivalent. |
| `1` | The patterns differ. |
| `2` | Invalid arguments or comparison failure. |

Results are written to stdout; errors are written to stderr.

## Installation

Install from a local checkout with Rust and Cargo:

```sh
cargo install --path .
```

To run from the checkout without installing:

```sh
cargo run --bin vk -- 'ab|ac' 'a(b|c)'
```

## Library

```rust
use replicant_regex::{Comparison, Pattern, Side};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let left = Pattern::new("a(b|c)")?;
    let right = Pattern::new("a(b|c)*")?;

    if let Comparison::Different(example) = left.compare(&right)? {
        assert_eq!(example.input, "a");
        assert_eq!(example.matched_by, Side::Right);
    }
    Ok(())
}
```

## Development

The project is written in Rust. To run the tests:

```sh
cargo test
```

Run the formatting, compilation, and Clippy checks with:

```sh
./scripts/check
```

The checks require the `rustfmt` and `clippy` components:

```sh
rustup component add rustfmt clippy
```

To run these checks before each commit, enable the Git hook in your clone:

```sh
git config --local core.hooksPath .githooks
```

The hook checks the working tree and rejects the commit if a check fails.
GitHub Actions runs the same checks and the tests on pushes and pull requests.

## Name

Replicant takes its name from *Blade Runner*. The film's Voight-Kampff test
distinguishes replicants from humans; this tool looks for an input that
distinguishes two regular expressions.

The command is named `vk`, after the Voight-Kampff test. The Rust package is
named `replicant-regex`.

## License

`replicant-regex` is distributed under the terms of both the MIT License and the Apache License 2.0.

See the [LICENSE-MIT][mit] and [LICENSE-APACHE][apache] files for more details.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

[mit]: LICENSE-MIT
[apache]: LICENSE-APACHE
