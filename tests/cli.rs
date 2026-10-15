use std::process::{Command, Output};

fn vk(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vk"))
        .args(args)
        .output()
        .unwrap()
}

fn assert_comparison(args: &[&str], code: i32, stdout: &str) {
    let output = vk(args);
    assert_eq!(output.status.code(), Some(code), "{output:?}");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), stdout);
    assert!(output.stderr.is_empty());
}

#[test]
fn equivalent_patterns_exit_zero() {
    assert_comparison(&["ab|ac", "a(b|c)"], 0, "equivalent\n");
    assert_comparison(&["", "a{0}"], 0, "equivalent\n");
}

#[test]
fn different_patterns_exit_one_and_report_the_accepting_side() {
    assert_comparison(
        &["a(b|c)", "a(b|c)*"],
        1,
        "different\ncounterexample: \"a\"\nmatches: right\n",
    );
    assert_comparison(
        &["a(b|c)*", "a(b|c)"],
        1,
        "different\ncounterexample: \"a\"\nmatches: left\n",
    );
}

#[test]
fn original_example_reports_the_empty_string() {
    assert_comparison(
        &["^(ab|ac)*$", "^a(b|c)*$"],
        1,
        "different\ncounterexample: \"\"\nmatches: left\n",
    );
}

#[test]
fn escapes_control_characters_quotes_and_backslashes() {
    for (pattern, escaped) in [
        (r"\n", r"\n"),
        (r"\r", r"\r"),
        (r"\t", r"\t"),
        (r"\x00", r"\0"),
        (r"\x1B", r"\u{1b}"),
        ("\"", "\\\""),
        (r"\\", r"\\"),
    ] {
        assert_comparison(
            &[pattern, "[a&&b]"],
            1,
            &format!("different\ncounterexample: \"{escaped}\"\nmatches: left\n"),
        );
    }
}

#[test]
fn preserves_readable_unicode() {
    assert_comparison(
        &["猫", "[a&&b]"],
        1,
        "different\ncounterexample: \"猫\"\nmatches: left\n",
    );
}

#[test]
fn double_dash_allows_patterns_that_look_like_options() {
    assert_comparison(&["--", "-a", "[-]a"], 0, "equivalent\n");
    assert_comparison(&["--", "--help", "--help"], 0, "equivalent\n");
}

#[test]
fn invalid_arguments_exit_two() {
    for args in [&[][..], &["a"], &["a", "b", "c"], &["--unknown"]] {
        let output = vk(args);
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("error:"), "{stderr}");
        assert!(stderr.contains("--help"), "{stderr}");
    }
}

#[test]
fn compile_errors_identify_the_pattern_and_exit_two() {
    for (args, side) in [(["[", "a"], "left"), (["a", "["], "right")] {
        let output = vk(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains(&format!("vk: {side} pattern:")), "{stderr}");
        assert!(stderr.contains("unclosed character class"), "{stderr}");
    }
}

#[test]
fn unsupported_patterns_exit_two() {
    for pattern in [r"\bcat\b", r"(a)\1", "a(?=b)", r"(?-u:.)"] {
        let output = vk(&[pattern, "a"]);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}

#[test]
fn compilation_limit_failure_exits_two() {
    let output = vk(&["a{1000000}", "a"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("left pattern:"), "{stderr}");
    assert!(stderr.contains("limit"), "{stderr}");
}

#[test]
fn help_and_version_exit_zero_without_comparing() {
    for flag in ["-h", "--help"] {
        let output = vk(&[flag]);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            stdout.starts_with(concat!("replicant ", env!("CARGO_PKG_VERSION"), "\n")),
            "{stdout}"
        );
        assert!(
            stdout.contains("Usage: vk [OPTIONS] <LEFT> <RIGHT>"),
            "{stdout}"
        );
        if flag == "--help" {
            assert!(stdout.contains("whole-string matching"), "{stdout}");
            assert!(stdout.contains("Exit status is 0"), "{stdout}");
        }
    }
    for flag in ["-V", "--version"] {
        assert_comparison(&[flag], 0, concat!("vk ", env!("CARGO_PKG_VERSION"), "\n"));
    }
}

#[cfg(unix)]
#[test]
fn rejects_non_utf8_arguments() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    let output = Command::new(env!("CARGO_BIN_EXE_vk"))
        .arg(OsString::from_vec(vec![0xFF]))
        .arg("a")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}

#[test]
fn compilation_limits_can_be_set_or_disabled() {
    for flag in [
        "--nfa-limit",
        "-n",
        "--dfa-limit",
        "-d",
        "--determinize-limit",
        "-t",
    ] {
        let output = vk(&[flag, "0", "a", "a"]);
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("left pattern:"), "{stderr}");
        assert!(stderr.contains("limit"), "{stderr}");
        for size in ["2097152", "2MB", "2MiB", "none"] {
            assert_comparison(&[flag, size, "ab|ac", "a(b|c)"], 0, "equivalent\n");
        }
    }
    assert_comparison(
        &[
            "--nfa-limit",
            "none",
            "--dfa-limit",
            "none",
            "--determinize-limit",
            "none",
            "--state-limit=none",
            "ab|ac",
            "a(b|c)",
        ],
        0,
        "equivalent\n",
    );
}

#[test]
fn state_pair_limits_are_enforced() {
    for flag in ["--state-limit", "-s"] {
        for limit in ["0", "1"] {
            let output = vk(&[flag, limit, "a", "b"]);
            assert_eq!(output.status.code(), Some(2));
            assert!(output.stdout.is_empty());
            let stderr = String::from_utf8(output.stderr).unwrap();
            assert!(
                stderr.contains(&format!("limit of {limit} state pairs")),
                "{stderr}"
            );
        }
        for limit in ["1000", "none"] {
            assert_comparison(
                &["a", "a|b", flag, limit],
                1,
                "different\ncounterexample: \"b\"\nmatches: right\n",
            );
        }
    }
    assert_comparison(
        &["-n2MiB", "-d2MiB", "-t2MiB", "-s1000", "ab|ac", "a(b|c)"],
        0,
        "equivalent\n",
    );
}

#[test]
fn invalid_limit_arguments_exit_two() {
    for flag in [
        "--nfa-limit",
        "--dfa-limit",
        "--determinize-limit",
        "--state-limit",
    ] {
        for value in [
            "-1",
            "1.5",
            "1XB",
            "1Kb",
            "1mib",
            "999999999999999999999999999999999999999999999999",
        ] {
            let argument = format!("{flag}={value}");
            let output = vk(&[&argument, "a", "b"]);
            assert_eq!(output.status.code(), Some(2));
            assert!(output.stdout.is_empty());
            let stderr = String::from_utf8(output.stderr).unwrap();
            assert!(stderr.contains(flag), "{stderr}");
        }
    }
    let output = vk(&["--state-limit", "1MiB", "a", "b"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

#[test]
fn json_reports_equivalence_and_differences() {
    for (args, code, expected) in [
        (
            ["--json", "ab|ac", "a(b|c)"],
            0,
            serde_json::json!({"equivalent": true, "counterexample": null, "matches": null}),
        ),
        (
            ["--json", "a(b|c)", "a(b|c)*"],
            1,
            serde_json::json!({"equivalent": false, "counterexample": "a", "matches": "right"}),
        ),
        (
            ["--json", "a(b|c)*", "a(b|c)"],
            1,
            serde_json::json!({"equivalent": false, "counterexample": "a", "matches": "left"}),
        ),
        (
            ["--json", "(ab|ac)*", "a(b|c)*"],
            1,
            serde_json::json!({"equivalent": false, "counterexample": "", "matches": "left"}),
        ),
    ] {
        let output = vk(&args);
        assert_eq!(output.status.code(), Some(code));
        assert!(output.stderr.is_empty());
        let actual: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(
            output.stdout.iter().filter(|&&byte| byte == b'\n').count(),
            1
        );
        assert!(output.stdout.ends_with(b"\n"));
    }
}

#[test]
fn json_round_trips_control_characters_and_unicode() {
    for (pattern, input) in [
        (r"\x00", "\0"),
        (r"\n", "\n"),
        (r"\t", "\t"),
        (r"\x1B", "\x1B"),
        ("\"", "\""),
        (r"\\", "\\"),
        ("猫", "猫"),
    ] {
        let output = vk(&["--json", "-d2MiB", "-s1000", pattern, "[a&&b]"]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stderr.is_empty());
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "equivalent": false,
                "counterexample": input,
                "matches": "left",
            })
        );
        assert_eq!(
            output.stdout.iter().filter(|&&byte| byte == b'\n').count(),
            1
        );
    }
}

#[test]
fn json_errors_leave_stdout_empty() {
    for args in [
        vec!["--json", "[", "a"],
        vec!["--json", "a", r"\bcat\b"],
        vec!["--json", "--state-limit", "0", "a", "b"],
        vec!["--json", "--nfa-limit", "0", "a", "b"],
        vec!["--json", "--dfa-limit", "1Kb", "a", "b"],
        vec!["--json", "a"],
    ] {
        let output = vk(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}
