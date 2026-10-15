//! Whole-string regular expressions for language comparison.

use std::{error::Error, fmt};

use regex_automata::{
    Anchored, Input, MatchKind,
    dfa::{Automaton, StartKind, dense},
    nfa::thompson,
    util::primitives::StateID,
};
use regex_syntax::hir::{Hir, Look};

const DEFAULT_COMPILE_SIZE_LIMIT: usize = 10 * 1024 * 1024;

/// Configuration for compiling a [`Pattern`].
///
/// Each size limit defaults to 10 MiB. `None` disables the corresponding limit.
/// The limits apply independently and do not bound total process memory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileOptions {
    /// The size limit, in bytes, for the nondeterministic finite automaton (NFA).
    pub nfa_size_limit: Option<usize>,
    /// The size limit, in bytes, for the deterministic finite automaton (DFA).
    pub dfa_size_limit: Option<usize>,
    /// The size limit, in bytes, for auxiliary memory used to construct the DFA.
    ///
    /// This excludes memory occupied by the DFA itself.
    pub determinize_size_limit: Option<usize>,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            nfa_size_limit: Some(DEFAULT_COMPILE_SIZE_LIMIT),
            dfa_size_limit: Some(DEFAULT_COMPILE_SIZE_LIMIT),
            determinize_size_limit: Some(DEFAULT_COMPILE_SIZE_LIMIT),
        }
    }
}

/// A compiled regular expression that matches entire UTF-8 strings.
///
/// A match must span the entire input, even when the expression has no anchors.
/// Unicode normalization is not performed.
///
/// Patterns use [Rust regex syntax]. Backreferences, lookaround, Unicode word
/// boundaries, and expressions that can match invalid UTF-8 are not supported.
/// ASCII word boundaries can be written as `(?-u:\b)` and `(?-u:\B)`.
///
/// [Rust regex syntax]: https://docs.rs/regex/latest/regex/#syntax
#[derive(Debug)]
pub struct Pattern {
    dfa: dense::DFA<Vec<u32>>,
    start: StateID,
}

impl Pattern {
    /// Creates a pattern using the default compilation options.
    ///
    /// See [`CompileOptions`] for the default limits and [`Pattern::with_options`]
    /// to configure them.
    ///
    /// # Errors
    ///
    /// Returns an error if the expression is invalid, uses an unsupported
    /// feature, or exceeds a compilation limit.
    ///
    /// # Examples
    ///
    /// ```
    /// use replicant::Pattern;
    ///
    /// let pattern = Pattern::new("ab|ac")?;
    /// assert!(pattern.is_match("ab"));
    /// assert!(!pattern.is_match("zab"));
    /// # Ok::<(), replicant::CompileError>(())
    /// ```
    pub fn new(pattern: &str) -> Result<Self, CompileError> {
        Self::with_options(pattern, &CompileOptions::default())
    }

    /// Creates a pattern using the given compilation options.
    ///
    /// # Errors
    ///
    /// Returns an error if the expression is invalid, uses an unsupported
    /// feature, or exceeds a configured compilation limit.
    ///
    /// # Examples
    ///
    /// ```
    /// use replicant::{CompileOptions, Pattern};
    ///
    /// let options = CompileOptions {
    ///     dfa_size_limit: Some(64 * 1024 * 1024),
    ///     ..CompileOptions::default()
    /// };
    /// let pattern = Pattern::with_options("a(b|c)*", &options)?;
    /// assert!(pattern.is_match("abc"));
    /// # Ok::<(), replicant::CompileError>(())
    /// ```
    pub fn with_options(pattern: &str, options: &CompileOptions) -> Result<Self, CompileError> {
        let hir = regex_syntax::Parser::new()
            .parse(pattern)
            .map_err(|error| CompileError::Syntax(error.to_string()))?;
        // Anchor the parsed expression instead of wrapping its source. In
        // extended mode, a trailing comment could otherwise swallow the end
        // anchor. Absolute anchors also remain absolute under multiline mode.
        let hir = Hir::concat(vec![Hir::look(Look::Start), hir, Hir::look(Look::End)]);
        let nfa = thompson::Compiler::new()
            .configure(
                thompson::Config::new()
                    .utf8(true)
                    .which_captures(thompson::WhichCaptures::None)
                    .nfa_size_limit(options.nfa_size_limit),
            )
            .build_from_hir(&hir)
            .map_err(|error| CompileError::Nfa(error.to_string()))?;
        let dfa = dense::Builder::new()
            .configure(
                dense::Config::new()
                    .start_kind(StartKind::Anchored)
                    .match_kind(MatchKind::All)
                    .unicode_word_boundary(false)
                    .dfa_size_limit(options.dfa_size_limit)
                    .determinize_size_limit(options.determinize_size_limit),
            )
            .build_from_nfa(&nfa)
            .map_err(|error| CompileError::Dfa(error.to_string()))?;
        // The builder enables anchored starts and has no quit bytes. With no
        // look-behind byte, this start configuration cannot fail.
        let start = dfa
            .start_state_forward(&Input::new("").anchored(Anchored::Yes))
            .expect("DFA supports anchored matching at the start of input");
        Ok(Self { dfa, start })
    }

    /// Returns `true` if the pattern matches the entire input.
    ///
    /// # Examples
    ///
    /// ```
    /// use replicant::Pattern;
    ///
    /// let pattern = Pattern::new("a*")?;
    /// assert!(pattern.is_match(""));
    /// assert!(pattern.is_match("aaa"));
    /// assert!(!pattern.is_match("aaab"));
    /// # Ok::<(), replicant::CompileError>(())
    /// ```
    pub fn is_match(&self, input: &str) -> bool {
        let mut state = self.start;
        for byte in input.bytes() {
            state = self.dfa.next_state(state, byte);
        }
        // regex-automata delays acceptance by one transition. End of input is
        // a distinct symbol, not a byte, and must be checked even for "".
        self.dfa.is_match_state(self.dfa.next_eoi_state(state))
    }
}

/// An error returned when a [`Pattern`] cannot be compiled.
///
/// Returned by [`Pattern::new`] and [`Pattern::with_options`].
#[derive(Debug)]
#[non_exhaustive]
pub enum CompileError {
    /// The expression could not be parsed.
    ///
    /// The string describes the syntax error or unsupported construct.
    Syntax(String),
    /// The nondeterministic finite automaton could not be constructed.
    ///
    /// The string describes the failure, such as exceeding the NFA size limit.
    Nfa(String),
    /// The deterministic finite automaton could not be constructed.
    ///
    /// The string describes the failure, such as an unsupported Unicode word
    /// boundary or an exceeded compilation limit.
    Dfa(String),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(error) => write!(f, "could not parse regex: {error}"),
            Self::Nfa(error) => write!(f, "could not build NFA: {error}"),
            Self::Dfa(error) => write!(f, "could not build DFA: {error}"),
        }
    }
}

impl Error for CompileError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(pattern: &str, accepted: &[&str], rejected: &[&str]) {
        let compiled = Pattern::new(pattern).unwrap();
        for input in accepted {
            assert!(compiled.is_match(input), "{pattern:?} rejected {input:?}");
        }
        for input in rejected {
            assert!(!compiled.is_match(input), "{pattern:?} accepted {input:?}");
        }
    }

    #[test]
    fn requires_the_whole_string() {
        check("ab|ac", &["ab", "ac"], &["", "a", "zab", "acz", "ab\n"]);
        check("^ab$", &["ab"], &["zab", "abz", "ab\n"]);
    }

    #[test]
    fn handles_empty_matches() {
        check("", &[""], &["a", "\n"]);
        check("a*", &["", "a", "aaa"], &["b", "aab"]);
        check("a|", &["", "a"], &["aa", "b"]);
    }

    #[test]
    fn distinguishes_nul_bytes_from_end_of_input() {
        check(r"a\x00", &["a\0"], &["a", "a\0b", "a\0\0"]);
    }

    #[test]
    fn handles_repetition_and_alternation() {
        check("(ab|ac)*", &["", "ab", "acab"], &["a", "abc", "abb"]);
        check("a(b|c)*", &["a", "ab", "acbc"], &["", "aba", "b"]);
        check("[a-c]{2,3}", &["ab", "ccc"], &["a", "abcd", "dddd"]);
        check("a|ab", &["a", "ab"], &["", "abc"]);
        check("a*?", &["", "a", "aaa"], &["b", "aab"]);
    }

    #[test]
    fn preserves_flags_without_weakening_whole_string_matching() {
        check("(?i)ab", &["ab", "AB", "aB"], &["xab", "ABx"]);
        check("(?m)^a$", &["a"], &["a\n", "\na", "b\na\nc"]);
        check("(?s:.)", &["a", "\n", "é"], &["", "ab"]);
    }

    #[test]
    fn trailing_extended_mode_comments_cannot_hide_anchors() {
        check("(?x)a # comment", &["a"], &["", "ab", "ba"]);
        check("(?x)# only a comment", &[""], &["a"]);
    }

    #[test]
    fn matches_unicode_without_normalization() {
        check("é|猫", &["é", "猫"], &["e\u{301}", "猫猫", "xé"]);
        check(r"\p{Greek}+", &["α", "Ωβ"], &["", "abc", "αx"]);
        check(".", &["é", "🦉"], &["", "\n", "ab"]);
    }

    #[test]
    fn accepts_ascii_word_boundaries() {
        check(r"(?-u:\b)[a-z]+(?-u:\b)", &["cat"], &["", " cat", "cat!"]);
    }

    #[test]
    fn rejects_invalid_and_unsupported_patterns() {
        for pattern in ["[", "(", r"(a)\1", "a(?=b)", "(?<=a)b", r"(?-u:.)"] {
            let error = Pattern::new(pattern).unwrap_err();
            assert!(
                matches!(error, CompileError::Syntax(_)),
                "{pattern}: {error}"
            );
            assert!(error.to_string().contains("could not parse regex"));
        }
    }

    #[test]
    fn rejects_unicode_word_boundaries_instead_of_approximating_them() {
        let error = Pattern::new(r"\bcat\b").unwrap_err();
        assert!(matches!(error, CompileError::Dfa(_)));
        assert!(error.to_string().contains("Unicode word"));
    }

    #[test]
    fn default_options_preserve_matching() {
        let options = CompileOptions::default();
        assert_eq!(options.nfa_size_limit, Some(10 * 1024 * 1024));
        assert_eq!(options.dfa_size_limit, Some(10 * 1024 * 1024));
        assert_eq!(options.determinize_size_limit, Some(10 * 1024 * 1024));
        let default = Pattern::new("a(b|c)*").unwrap();
        let explicit = Pattern::with_options("a(b|c)*", &options).unwrap();
        for input in ["", "a", "abc", "acbc", "aba"] {
            assert_eq!(default.is_match(input), explicit.is_match(input));
        }
    }

    #[test]
    fn compilation_limits_can_be_disabled() {
        let options = CompileOptions {
            nfa_size_limit: None,
            dfa_size_limit: None,
            determinize_size_limit: None,
        };
        let pattern = Pattern::with_options("ab|ac", &options).unwrap();
        assert!(pattern.is_match("ab"));
        assert!(!pattern.is_match("abc"));
    }

    #[test]
    fn each_compilation_limit_is_enforced_independently() {
        for stage in 0..3 {
            let mut options = CompileOptions {
                nfa_size_limit: None,
                dfa_size_limit: None,
                determinize_size_limit: None,
            };
            match stage {
                0 => options.nfa_size_limit = Some(0),
                1 => options.dfa_size_limit = Some(0),
                _ => options.determinize_size_limit = Some(0),
            }
            let error = Pattern::with_options("ab|ac", &options).unwrap_err();
            if stage == 0 {
                assert!(matches!(error, CompileError::Nfa(_)), "{error}");
            } else {
                assert!(matches!(error, CompileError::Dfa(_)), "{error}");
            }
            assert!(error.to_string().contains("limit"), "{error}");
        }
    }

    #[test]
    fn syntax_diagnostics_preserve_pattern_and_reason() {
        let error = Pattern::new("[abc").unwrap_err();
        let CompileError::Syntax(details) = &error else {
            panic!("expected syntax error: {error}");
        };
        assert!(details.contains("[abc"), "{details}");
        assert!(details.contains("unclosed character class"), "{details}");
        assert!(error.source().is_none());
    }
}
