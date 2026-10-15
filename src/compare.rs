use std::{collections::HashSet, error::Error, fmt};

use regex_automata::{
    dfa::{Automaton, dense},
    util::primitives::StateID,
};

/// The result of comparing the languages of two patterns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Comparison {
    /// Both patterns match exactly the same strings.
    Equivalent,
    /// The patterns differ on the given counterexample.
    Different(Counterexample),
}

/// A shortest string matched by exactly one of the compared patterns.
///
/// Length is measured in UTF-8 bytes. When several strings have the same minimum
/// length, any one of them may be returned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Counterexample {
    /// The input that distinguishes the patterns. May be empty.
    pub input: String,
    /// The pattern that matches the input.
    pub matched_by: Side,
}

/// A pattern's position in a comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// The pattern on which the comparison method was called.
    Left,
    /// The pattern passed as the `other` argument.
    Right,
}

/// Configuration for comparing two patterns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompareOptions {
    /// The maximum number of distinct DFA state pairs stored during comparison.
    ///
    /// Defaults to 100,000 pairs. The initial pair counts toward the limit.
    /// `None` disables the limit.
    pub state_pair_limit: Option<usize>,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            state_pair_limit: Some(100_000),
        }
    }
}

/// An error returned when a pattern comparison cannot be completed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompareError {
    /// The comparison required more state pairs than the configured limit.
    StateLimitExceeded {
        /// The configured state pair limit.
        limit: usize,
    },
}

impl fmt::Display for CompareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StateLimitExceeded { limit } => {
                write!(f, "comparison exceeded the limit of {limit} state pairs")
            }
        }
    }
}

impl Error for CompareError {}

pub(crate) fn compare(
    left_dfa: &dense::DFA<Vec<u32>>,
    left_start: StateID,
    right_dfa: &dense::DFA<Vec<u32>>,
    right_start: StateID,
    options: &CompareOptions,
) -> Result<Comparison, CompareError> {
    if options.state_pair_limit == Some(0) {
        return Err(CompareError::StateLimitExceeded { limit: 0 });
    }

    let start = (left_start, right_start);
    let mut visited = HashSet::from([start]);
    let mut nodes = vec![SearchNode {
        states: start,
        predecessor: None,
    }];
    // Appending successors and visiting nodes in insertion order gives BFS.
    let mut index = 0;
    while index < nodes.len() {
        let (left, right) = nodes[index].states;
        // Acceptance is delayed by one transition. Only the EOI transition
        // tells us whether a string ending at this pair distinguishes them.
        let left_accepts = left_dfa.is_match_state(left_dfa.next_eoi_state(left));
        let right_accepts = right_dfa.is_match_state(right_dfa.next_eoi_state(right));
        if left_accepts != right_accepts {
            return Ok(Comparison::Different(Counterexample {
                input: reconstruct(&nodes, index),
                matched_by: if left_accepts {
                    Side::Left
                } else {
                    Side::Right
                },
            }));
        }

        // Patterns can only accept valid UTF-8, so invalid byte strings
        // cannot distinguish them. Byte transitions still cover every
        // possible UTF-8 string, including multibyte characters.
        for byte in u8::MIN..=u8::MAX {
            let next = (
                left_dfa.next_state(left, byte),
                right_dfa.next_state(right, byte),
            );
            if visited.contains(&next) {
                continue;
            }
            if let Some(limit) = options.state_pair_limit
                && nodes.len() >= limit
            {
                return Err(CompareError::StateLimitExceeded { limit });
            }
            visited.insert(next);
            nodes.push(SearchNode {
                states: next,
                predecessor: Some((index, byte)),
            });
        }
        index += 1;
    }

    Ok(Comparison::Equivalent)
}

struct SearchNode {
    states: (StateID, StateID),
    predecessor: Option<(usize, u8)>,
}

fn reconstruct(nodes: &[SearchNode], mut index: usize) -> String {
    let mut bytes = Vec::new();
    while let Some((parent, byte)) = nodes[index].predecessor {
        bytes.push(byte);
        index = parent;
    }
    bytes.reverse();
    // The accepting pattern was compiled to match only valid UTF-8 strings.
    String::from_utf8(bytes).expect("a counterexample accepted by a pattern is valid UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Pattern;

    fn compare(left: &str, right: &str, expected: bool) {
        let left = Pattern::new(left).unwrap();
        let right = Pattern::new(right).unwrap();
        assert_eq!(left.is_equivalent(&right).unwrap(), expected);
        assert_eq!(right.is_equivalent(&left).unwrap(), expected);
        for (left, right) in [(&left, &right), (&right, &left)] {
            match left.compare(right).unwrap() {
                Comparison::Equivalent => assert!(expected),
                Comparison::Different(example) => {
                    assert!(!expected);
                    assert_ne!(
                        left.is_match(&example.input),
                        right.is_match(&example.input)
                    );
                    assert_eq!(
                        left.is_match(&example.input),
                        example.matched_by == Side::Left
                    );
                }
            }
        }
    }

    #[test]
    fn equivalent_expressions() {
        for (left, right) in [
            ("ab|ac", "a(b|c)"),
            ("a", "^a$"),
            ("[a-c]", "a|b|c"),
            ("a*", "(?:aa*|)"),
            ("a*?", "a*"),
            ("a|ab", "ab?"),
            ("(a+)", "a+"),
            ("(?i:a)", "[aA]"),
            ("(?x)a # comment", "a"),
            ("(?m)^a$", "a"),
        ] {
            compare(left, right, true);
        }
    }

    #[test]
    fn different_expressions() {
        for (left, right) in [
            ("^(ab|ac)*$", "^a(b|c)*$"),
            ("a", "b"),
            ("a*", "a+"),
            ("ab", "abc"),
            ("a", "a|ab"),
            ("a{40}", "a{41}"),
            (".", "(?s:.)"),
            (r"a\x00", "a"),
            (r"[\x00-\xFE]", r"[\x00-\xFF]"),
        ] {
            compare(left, right, false);
        }
    }

    #[test]
    fn empty_strings_and_empty_languages() {
        compare("", "a{0}", true);
        compare("", "a", false);
        compare("", "a*", false);
        compare("[a&&b]", "[^\\s\\S]", true);
        compare("[a&&b]", "", false);
        compare("[a&&b]", "a", false);
    }

    #[test]
    fn equivalent_cycles_terminate() {
        compare("(a|b)*", "[ab]*", true);
        compare("(?s:.)*", "(?s:.)*", true);
    }

    #[test]
    fn compares_unicode_languages() {
        compare("é|猫", "[é猫]", true);
        compare("é", "e\u{301}", false);
        compare("🦉", "🦊", false);
        compare(r"\p{Greek}", r"[\p{Greek}]", true);
        compare(r"\w", r"(?-u:\w)", false);
        compare(".", r"[\x00-\x09\x0B-\x7F]", false);
    }

    #[test]
    fn compares_ascii_word_boundaries() {
        compare(r"(?-u:\b)cat(?-u:\b)", "cat", true);
        compare(r"(?-u:\b)é", "é", false);
    }

    #[test]
    fn zero_limit_cannot_store_the_initial_pair() {
        let pattern = Pattern::new("a").unwrap();
        let options = CompareOptions {
            state_pair_limit: Some(0),
        };
        assert_eq!(
            pattern.is_equivalent_with_options(&pattern, &options),
            Err(CompareError::StateLimitExceeded { limit: 0 }),
        );
        assert_eq!(
            pattern.compare_with_options(&pattern, &options),
            Err(CompareError::StateLimitExceeded { limit: 0 }),
        );
    }

    #[test]
    fn exhaustion_does_not_report_equivalence_or_inequivalence() {
        let left = Pattern::new("a").unwrap();
        let options = CompareOptions {
            state_pair_limit: Some(1),
        };
        for right in ["a", "b"] {
            let right = Pattern::new(right).unwrap();
            assert_eq!(
                left.is_equivalent_with_options(&right, &options),
                Err(CompareError::StateLimitExceeded { limit: 1 }),
            );
            assert_eq!(
                left.compare_with_options(&right, &options),
                Err(CompareError::StateLimitExceeded { limit: 1 }),
            );
        }
    }

    #[test]
    fn empty_string_difference_needs_only_the_initial_pair() {
        let left = Pattern::new("").unwrap();
        let right = Pattern::new("a").unwrap();
        let options = CompareOptions {
            state_pair_limit: Some(1),
        };
        assert!(!left.is_equivalent_with_options(&right, &options).unwrap());
        assert_eq!(
            left.compare_with_options(&right, &options).unwrap(),
            Comparison::Different(Counterexample {
                input: String::new(),
                matched_by: Side::Left,
            }),
        );
    }

    #[test]
    fn limits_can_be_increased_or_disabled() {
        let left = Pattern::new("(ab|ac)*").unwrap();
        for limit in [Some(100), None] {
            let options = CompareOptions {
                state_pair_limit: limit,
            };
            let same = Pattern::new("(a[bc])*").unwrap();
            let different = Pattern::new("a(b|c)*").unwrap();
            assert!(left.is_equivalent_with_options(&same, &options).unwrap());
            assert!(
                !left
                    .is_equivalent_with_options(&different, &options)
                    .unwrap()
            );
        }
    }

    #[test]
    fn reconstructs_shortest_witnesses_and_the_accepting_side() {
        for (left, right, input, matched_by) in [
            ("(ab|ac)*", "a(b|c)*", "", Side::Left),
            ("a", "a|ab", "ab", Side::Right),
            ("a*b", "a*c", "b", Side::Left),
            ("[^\\s\\S]", "🦉", "🦉", Side::Right),
            (".", "(?s:.)", "\n", Side::Right),
            (r"a\x00", "[^\\s\\S]", "a\0", Side::Left),
        ] {
            let left = Pattern::new(left).unwrap();
            let right = Pattern::new(right).unwrap();
            assert_eq!(
                left.compare(&right).unwrap(),
                Comparison::Different(Counterexample {
                    input: input.to_owned(),
                    matched_by,
                }),
            );
            assert_eq!(
                right.compare(&left).unwrap(),
                Comparison::Different(Counterexample {
                    input: input.to_owned(),
                    matched_by: match matched_by {
                        Side::Left => Side::Right,
                        Side::Right => Side::Left,
                    },
                }),
            );
        }
    }

    #[test]
    fn shortest_means_bytes_rather_than_characters() {
        let left = Pattern::new("猫|ab").unwrap();
        let right = Pattern::new("[a&&b]").unwrap();
        assert_eq!(
            left.compare(&right).unwrap(),
            Comparison::Different(Counterexample {
                input: "ab".to_owned(),
                matched_by: Side::Left,
            }),
        );
    }

    #[test]
    fn reconstructs_long_witnesses() {
        let left = Pattern::new("a{40}").unwrap();
        let right = Pattern::new("a{41}").unwrap();
        assert_eq!(
            left.compare(&right).unwrap(),
            Comparison::Different(Counterexample {
                input: "a".repeat(40),
                matched_by: Side::Left,
            }),
        );
    }

    #[test]
    fn no_shorter_utf8_input_distinguishes_the_patterns() {
        // Exhaust all byte strings shorter than each witness, checking valid
        // UTF-8 inputs with the separate whole-string matching operation.
        fn check_shorter(left: &Pattern, right: &Pattern, bytes: &mut Vec<u8>, length: usize) {
            if bytes.len() >= length {
                return;
            }
            if let Ok(input) = std::str::from_utf8(bytes) {
                assert_eq!(left.is_match(input), right.is_match(input), "{input:?}");
            }
            if bytes.len() + 1 < length {
                for byte in u8::MIN..=u8::MAX {
                    bytes.push(byte);
                    check_shorter(left, right, bytes, length);
                    bytes.pop();
                }
            }
        }

        for (left, right, length) in [
            ("abc", "abd", 3),
            ("猫", "[a&&b]", 3),
            ("é|ab", "[a&&b]", 2),
            (r"a\x00", "[a&&b]", 2),
            ("[ab]*a[ab]{2}", "[a&&b]", 3),
        ] {
            let left = Pattern::new(left).unwrap();
            let right = Pattern::new(right).unwrap();
            let Comparison::Different(example) = left.compare(&right).unwrap() else {
                panic!("expected a counterexample");
            };
            assert_eq!(example.input.len(), length);
            assert_ne!(
                left.is_match(&example.input),
                right.is_match(&example.input)
            );
            check_shorter(&left, &right, &mut Vec::new(), length);
        }
    }
}
