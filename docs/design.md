# Design

This document describes the intended first version. Compilation, whole-string
matching, equivalence checking, and shortest counterexamples are implemented.
The CLI is not. The executable will be named `vk`; the Rust package is named
`replicant`.

## Matching semantics

- **Whole-string matching.** Each expression must accept the entire input.
  Explicit anchors are optional: `a` and `^a$` describe the same language.
  An expression matching a substring does not make the entire input a match.
- **UTF-8 strings.** The comparison domain is all valid UTF-8 strings, including
  the empty string. Unicode normalization is not performed: composed and
  decomposed spellings are distinct inputs.
- **Rust regex syntax, with a documented subset.** The implementation will use
  `regex-automata`. The initial scope includes literals, character classes,
  concatenation, alternation, grouping, repetition, anchors, Unicode character
  classes, and flags supported by its DFA engine. Backreferences, lookaround,
  Unicode word boundaries, and expressions that can match invalid UTF-8 are
  unsupported and must produce errors. ASCII word boundaries may be used with
  Unicode mode disabled for those assertions, such as `(?-u:\b)`.
- **Shortest counterexamples.** A counterexample must be accepted by exactly one
  expression. Shortest means fewest UTF-8 bytes, not fewest Unicode characters
  or grapheme clusters. Any witness tied for shortest is acceptable. Output must
  escape control characters and make the empty string visible as `""`.
- **Exact answers or explicit errors.** Equivalence must be proved by exhausting
  the reachable search space. Invalid syntax, unsupported constructs, and
  resource exhaustion are errors, never evidence of equivalence.
- **Bounded resources.** DFA construction and the number of visited state pairs
  will have finite limits. Hitting a limit must report that the comparison could
  not be completed.

Compilation defaults to a 10 MiB limit for each of the NFA, DFA, and auxiliary
determinization storage. Callers can change these independently through `CompileOptions`
and `Pattern::with_options`. `Some(bytes)` sets a limit; `None` disables it.
These are separate limits, not a cap on total process memory.

`CompileError` distinguishes syntax, NFA construction, and DFA construction
failures. Each variant holds a diagnostic string; dependency error types are not
exposed. Diagnostic wording is not a stable interface.

`Pattern::is_equivalent` returns whether two compiled patterns match exactly the
same UTF-8 strings. It searches pairs of DFA states in breadth-first order,
checking acceptance at end of input at each pair. Invalid UTF-8 cannot be
accepted by either compiled pattern and therefore cannot distinguish them.

Comparison defaults to a limit of 100,000 distinct state pairs, including the
initial pair. `CompareOptions::state_pair_limit` changes or disables this limit
through `Pattern::is_equivalent_with_options` or `Pattern::compare_with_options`.
Exceeding it returns
`CompareError::StateLimitExceeded`. Equivalence is reported only after all
reachable pairs have been explored; finding different acceptance reports
inequivalence.

`Pattern::compare` returns `Comparison::Equivalent` or
`Comparison::Different(Counterexample)`. A counterexample contains a UTF-8 input
and the `Side` that accepts it: `Left` is the method receiver, and `Right` is the
other pattern. The search stores a predecessor and byte for each discovered
state pair. Following these links reconstructs a shortest distinguishing string
in bytes, including the empty string when it distinguishes the patterns.

`Pattern::is_equivalent` delegates to the same search and returns only the
equivalence decision. Library inputs and counterexamples are ordinary strings;
escaping them for terminal output belongs to the CLI.

The planned CLI accepts two patterns and uses these exit codes:

| Code | Meaning |
| --- | --- |
| `0` | The patterns recognize the same language. |
| `1` | The patterns differ; output includes a witness and which pattern accepts it. |
| `2` | Invalid arguments or comparison failure. |

## Approach

Compile each expression into a deterministic finite automaton (DFA), enforcing
whole-string matching. Search pairs of states in breadth-first order. A pair
where exactly one automaton accepts at end of input identifies a counterexample;
predecessor links reconstruct a shortest witness. Exhausting all reachable pairs
proves equivalence. This explores the symmetric difference of the languages
without generating random test strings.

## Incremental implementation

1. Define the package name and initial contract (this scaffold).
2. Compile a single expression into a DFA with whole-string semantics.
3. Decide equivalence by traversing reachable state pairs.
4. Reconstruct a shortest counterexample and identify the accepting pattern.
5. Enforce compilation and search limits with explicit errors.
6. Add the CLI, escaped output, help, and exit codes.
7. Cross-check small cases against an independent matcher and document usage.

DFA minimization, additional regex dialects, visualizations, and performance
tuning are deferred until the core comparison works.

