/// An explicit limit, including `None` to disable it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Limit(pub Option<usize>);

pub(super) fn parse_size_limit(value: &str) -> Result<Limit, String> {
    if value.eq_ignore_ascii_case("none") {
        return Ok(Limit(None));
    }
    let split = value
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(value.len());
    let (number, unit) = value.split_at(split);
    let multiplier: u128 = match unit {
        "" | "B" => 1,
        "KB" => 1_000,
        "MB" => 1_000_000,
        "GB" => 1_000_000_000,
        "TB" => 1_000_000_000_000,
        "KiB" => 1 << 10,
        "MiB" => 1 << 20,
        "GiB" => 1 << 30,
        "TiB" => 1 << 40,
        _ => {
            return Err(
                "expected bytes or a size with B, KB, MB, GB, TB, KiB, MiB, GiB, or TiB".to_owned(),
            );
        }
    };
    let bytes = number
        .parse::<u128>()
        .ok()
        .and_then(|n| n.checked_mul(multiplier))
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| "size is invalid or too large; use a non-negative integer with an optional unit, or 'none'".to_owned())?;
    Ok(Limit(Some(bytes)))
}

pub(super) fn parse_state_limit(value: &str) -> Result<Limit, String> {
    if value.eq_ignore_ascii_case("none") {
        return Ok(Limit(None));
    }
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("expected a non-negative integer or 'none'".to_owned());
    }
    value
        .parse::<usize>()
        .map(|n| Limit(Some(n)))
        .map_err(|_| "state pair limit is too large".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sizes_without_rounding() {
        for (value, expected) in [
            ("0", 0),
            ("123", 123),
            ("42B", 42),
            ("2KB", 2_000),
            ("2MB", 2_000_000),
            ("1GB", 1_000_000_000),
            ("2KiB", 2_048),
            ("64MiB", 64 * 1024 * 1024),
            ("1GiB", 1 << 30),
        ] {
            assert_eq!(
                parse_size_limit(value).unwrap().0,
                Some(expected),
                "{value}"
            );
        }
        assert_eq!(
            parse_size_limit(&usize::MAX.to_string()).unwrap().0,
            Some(usize::MAX)
        );
    }

    #[test]
    fn parses_terabytes_when_the_platform_can_represent_them() {
        for (value, bytes) in [("1TB", 1_000_000_000_000u128), ("1TiB", 1 << 40)] {
            match usize::try_from(bytes) {
                Ok(bytes) => assert_eq!(parse_size_limit(value).unwrap().0, Some(bytes)),
                Err(_) => assert!(parse_size_limit(value).is_err()),
            }
        }
    }

    #[test]
    fn distinguishes_disabled_and_zero_limits() {
        for parse in [parse_size_limit, parse_state_limit] {
            assert_eq!(parse("none").unwrap().0, None);
            assert_eq!(parse("NONE").unwrap().0, None);
            assert_eq!(parse("0").unwrap().0, Some(0));
        }
        assert_eq!(parse_state_limit("100000").unwrap().0, Some(100_000));
    }

    #[test]
    fn rejects_malformed_sizes_and_overflow() {
        for value in [
            "",
            "MiB",
            "-1",
            "+1",
            "1.5MiB",
            "1 MiB",
            "1_000",
            "1XB",
            "1é",
            "unlimited",
        ] {
            assert!(parse_size_limit(value).is_err(), "{value}");
        }
        for value in [
            format!("{}0", usize::MAX),
            format!("{}KiB", usize::MAX),
            format!("{}TiB", u128::MAX),
        ] {
            assert!(parse_size_limit(&value).is_err(), "{value}");
        }
    }

    #[test]
    fn rejects_bit_units_and_incorrect_capitalization() {
        for value in [
            "1b", "1Kb", "1kb", "1MBi", "1mb", "1KIB", "1mIb", "1mib", "1Gib", "1tib",
        ] {
            assert!(parse_size_limit(value).is_err(), "{value}");
        }
    }

    #[test]
    fn state_limits_require_counts_instead_of_sizes() {
        for value in ["", "-1", "+1", "1.5", "1KiB", "1_000", " 1", "1 "] {
            assert!(parse_state_limit(value).is_err(), "{value}");
        }
        assert!(parse_state_limit(&format!("{}0", usize::MAX)).is_err());
    }
}
