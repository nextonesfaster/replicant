# Replicant

Replicant is an experiment in checking whether two regular expressions match
exactly the same strings, and finding a shortest counterexample when they differ.

Comparison and the command-line interface are not implemented yet. See the
[design notes](docs/design.md) for matching semantics and the implementation plan.

## Development

The project is written in Rust. To run the tests:

```sh
cargo test
```

## Name

Replicant takes its name from *Blade Runner*. The film's Voight-Kampff test
distinguishes replicants from humans; this tool looks for an input that
distinguishes two regular expressions.

The command is named `vk`, after the Voight-Kampff test. The Rust package is
named `replicant`.

## License

`replicant` is distributed under the terms of both the MIT License and the Apache License 2.0.

See the [LICENSE-MIT][mit] and [LICENSE-APACHE][apache] files for more details.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

[mit]: LICENSE-MIT
[apache]: LICENSE-APACHE
