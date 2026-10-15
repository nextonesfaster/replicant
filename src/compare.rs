use std::{
    collections::{HashSet, VecDeque},
    error::Error,
    fmt,
};

use regex_automata::{
    dfa::{Automaton, dense},
    util::primitives::StateID,
};

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

pub(crate) fn is_equivalent(
    left_dfa: &dense::DFA<Vec<u32>>,
    left_start: StateID,
    right_dfa: &dense::DFA<Vec<u32>>,
    right_start: StateID,
    options: &CompareOptions,
) -> Result<bool, CompareError> {
    if options.state_pair_limit == Some(0) {
        return Err(CompareError::StateLimitExceeded { limit: 0 });
    }

    let start = (left_start, right_start);
    let mut visited = HashSet::from([start]);
    let mut queue = VecDeque::from([start]);

    while let Some((left, right)) = queue.pop_front() {
        // Acceptance is delayed by one transition. Only the EOI transition
        // tells us whether a string ending at this pair distinguishes them.
        let left_accepts = left_dfa.is_match_state(left_dfa.next_eoi_state(left));
        let right_accepts = right_dfa.is_match_state(right_dfa.next_eoi_state(right));
        if left_accepts != right_accepts {
            return Ok(false);
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
                && visited.len() >= limit
            {
                return Err(CompareError::StateLimitExceeded { limit });
            }
            visited.insert(next);
            queue.push_back(next);
        }
    }

    Ok(true)
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
}
