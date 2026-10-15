use std::collections::BTreeSet;

use replicant::{CompareError, CompareOptions, Comparison, Pattern, Side};

struct FinitePattern {
    source: String,
    language: BTreeSet<String>,
    compiled: Pattern,
}

impl FinitePattern {
    fn new(source: String, language: BTreeSet<String>) -> Self {
        let compiled = Pattern::new(&source).unwrap();
        Self {
            source,
            language,
            compiled,
        }
    }
}

#[test]
fn generated_finite_languages_agree_with_set_operations() {
    // Construct expected languages directly, without parsing or matching regexes.
    // These expressions cannot accept strings outside their finite sets, so the
    // oracle checks exact equivalence and minimum byte length over all inputs.
    let atoms = [
        ("", vec![""]),
        ("a", vec!["a"]),
        ("b", vec!["b"]),
        ("é", vec!["é"]),
        (r"\x00", vec!["\0"]),
        ("[a&&b]", vec![]),
    ];
    let mut patterns: Vec<_> = atoms
        .into_iter()
        .map(|(source, language)| {
            FinitePattern::new(
                source.to_owned(),
                language.into_iter().map(str::to_owned).collect(),
            )
        })
        .collect();
    let atom_count = patterns.len();
    for left in 0..atom_count {
        for right in 0..atom_count {
            let left = &patterns[left];
            let right = &patterns[right];
            let union = FinitePattern::new(
                format!("(?:{}|{})", left.source, right.source),
                left.language.union(&right.language).cloned().collect(),
            );
            let concatenation = FinitePattern::new(
                format!("(?:{})(?:{})", left.source, right.source),
                left.language
                    .iter()
                    .flat_map(|a| right.language.iter().map(move |b| format!("{a}{b}")))
                    .collect(),
            );
            patterns.extend([union, concatenation]);
        }
        let atom = &patterns[left];
        let mut optional_language = atom.language.clone();
        optional_language.insert(String::new());
        let optional = FinitePattern::new(format!("(?:{})?", atom.source), optional_language);
        let mut repeated_language = atom.language.clone();
        repeated_language.insert(String::new());
        repeated_language.extend(
            atom.language
                .iter()
                .flat_map(|a| atom.language.iter().map(move |b| format!("{a}{b}"))),
        );
        let repeated = FinitePattern::new(format!("(?:{}){{0,2}}", atom.source), repeated_language);
        patterns.extend([optional, repeated]);
    }

    for (index, left) in patterns.iter().enumerate() {
        for right in &patterns[index..] {
            let minimum_length = left
                .language
                .symmetric_difference(&right.language)
                .map(String::len)
                .min();
            let result = left.compiled.compare(&right.compiled).unwrap();
            match result {
                Comparison::Equivalent => {
                    assert_eq!(
                        left.language, right.language,
                        "{} vs {}",
                        left.source, right.source
                    );
                }
                Comparison::Different(example) => {
                    assert_eq!(
                        Some(example.input.len()),
                        minimum_length,
                        "{} vs {}: {example:?}",
                        left.source,
                        right.source,
                    );
                    let left_matches = left.language.contains(&example.input);
                    let right_matches = right.language.contains(&example.input);
                    assert_ne!(left_matches, right_matches, "{example:?}");
                    assert_eq!(left_matches, example.matched_by == Side::Left);
                }
            }
        }
    }
}

#[test]
fn witnesses_cover_utf8_encoding_boundaries() {
    let empty = Pattern::new("[a&&b]").unwrap();
    // Include both sides of UTF-8 length changes, the surrogate gap, and the
    // largest scalar value. Every character is the unique witness for its pair.
    for character in [
        '\0',
        '\u{7F}',
        '\u{80}',
        '\u{7FF}',
        '\u{800}',
        '\u{D7FF}',
        '\u{E000}',
        '\u{FFFF}',
        '\u{10000}',
        '\u{10FFFF}',
    ] {
        let input = character.to_string();
        let pattern = Pattern::new(&format!(r"\x{{{:X}}}", u32::from(character))).unwrap();
        for (left, right, matched_by) in [
            (&pattern, &empty, Side::Left),
            (&empty, &pattern, Side::Right),
        ] {
            let Comparison::Different(example) = left.compare(right).unwrap() else {
                panic!("expected a witness for {character:?}");
            };
            assert_eq!(example.input, input);
            assert_eq!(example.matched_by, matched_by);
        }
    }
}

#[test]
fn assertions_preserve_whole_string_semantics() {
    for (left, right) in [
        (r"\Aa\z", "a"),
        (r"a\Ab", "[a&&b]"),
        (r"a\zb", "[a&&b]"),
        (r"(?-u:\B)é(?-u:\B)", "é"),
        (r"(?-u:\B)a", "[a&&b]"),
        (r"(?m:^a$\n^b$)", "a\nb"),
        (r"(?mR:^a$\r\n^b$)", "a\r\nb"),
    ] {
        let left_pattern = Pattern::new(left).unwrap();
        let right_pattern = Pattern::new(right).unwrap();
        assert_eq!(
            left_pattern.compare(&right_pattern).unwrap(),
            Comparison::Equivalent,
            "{left:?} vs {right:?}"
        );
    }
}

#[test]
fn comparisons_can_be_retried_after_exhausting_the_limit() {
    let left = Pattern::new("abc").unwrap();
    let right = Pattern::new("abd").unwrap();
    let options = CompareOptions {
        state_pair_limit: Some(1),
    };
    assert!(matches!(
        left.compare_with_options(&right, &options),
        Err(CompareError::StateLimitExceeded { limit: 1 }),
    ));
    let Comparison::Different(example) = left.compare(&right).unwrap() else {
        panic!("expected a counterexample after retrying");
    };
    match example.matched_by {
        Side::Left => assert_eq!(example.input, "abc"),
        Side::Right => assert_eq!(example.input, "abd"),
    }
}

#[test]
fn algebraic_identities_hold_for_infinite_languages() {
    for source in ["a", "ab|a", "a*", "[ab]", "é|猫", "(?:a?b)*"] {
        for (left, right) in [
            (
                format!("(?:{source})*"),
                format!("(?:{source})*(?:{source})*"),
            ),
            (
                format!("(?:{source})+"),
                format!("(?:{source})(?:{source})*"),
            ),
            (format!("(?:{source})?"), format!("(?:|{source})")),
        ] {
            let left_pattern = Pattern::new(&left).unwrap();
            let right_pattern = Pattern::new(&right).unwrap();
            assert_eq!(
                left_pattern.compare(&right_pattern).unwrap(),
                Comparison::Equivalent,
                "{left:?} vs {right:?}"
            );
        }
    }
}
