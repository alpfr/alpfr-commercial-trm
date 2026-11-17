use std::cmp::Ordering;
use std::fmt;

/// A version as it appears in the wild: `16`, `16.4`, `v1.33.2`, `3.12.0rc1`,
/// `17.0-beta2`, `16.4-alpine`, `21.0.2+13`.
///
/// Missing trailing segments count as zero, so `16 == 16.0 == 16.0.0`.
/// A pre-release sorts before the release with the same numbers.
#[derive(Debug, Clone)]
pub struct Version {
    segments: Vec<u64>,
    pre: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionError(pub String);

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid version: {}", self.0)
    }
}

impl std::error::Error for VersionError {}

/// Suffixes that mean "not yet the release". Anything else after the numbers
/// (`-alpine`, ` LTSC`, `u412`) is a variant label and is ignored.
const PRE_MARKERS: &[&str] = &[
    "alpha", "beta", "rc", "preview", "pre", "dev", "snapshot", "milestone", "nightly", "canary", "ea",
];

fn is_pre_release(suffix: &str) -> bool {
    // The marker must stand alone or be followed by a number or separator, so
    // "rc1" and "beta.2" qualify but "precise" and "eap" do not.
    let stands_alone = |rest: &str| rest.chars().next().is_none_or(|c| c.is_ascii_digit() || ".-_".contains(c));
    if PRE_MARKERS.iter().any(|m| suffix.strip_prefix(m).is_some_and(stands_alone)) {
        return true;
    }
    // PEP 440 and Maven shorthands: a1, b2, m3
    let mut chars = suffix.chars();
    matches!(chars.next(), Some('a' | 'b' | 'm')) && chars.next().is_some_and(|c| c.is_ascii_digit())
}

impl Version {
    pub fn parse(input: &str) -> Result<Self, VersionError> {
        let err = || VersionError(input.to_owned());
        let mut s = input.trim();
        if let Some(rest) = s.strip_prefix(['v', 'V']) {
            if rest.starts_with(|c: char| c.is_ascii_digit()) {
                s = rest;
            }
        }
        // Build metadata never affects ordering.
        let s = s.split('+').next().unwrap_or("");

        let core_end = s.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(s.len());
        let core = s[..core_end].trim_end_matches('.');
        if core.is_empty() {
            return Err(err());
        }
        let segments = core
            .split('.')
            .map(|seg| seg.parse::<u64>().map_err(|_| err()))
            .collect::<Result<Vec<_>, _>>()?;

        let suffix = s[core_end..]
            .trim_start_matches(['-', '_', '.', ' '])
            .trim()
            .to_ascii_lowercase();
        let pre = (!suffix.is_empty() && is_pre_release(&suffix)).then_some(suffix);
        Ok(Version { segments, pre })
    }

    /// Numeric segment at `index`, zero when absent.
    pub fn segment(&self, index: usize) -> u64 {
        self.segments.get(index).copied().unwrap_or(0)
    }

    pub fn segments(&self) -> &[u64] {
        &self.segments
    }

    pub fn is_pre_release(&self) -> bool {
        self.pre.is_some()
    }

    /// True when both versions have the same numbers, ignoring any pre-release.
    pub fn same_release(&self, other: &Version) -> bool {
        self.cmp_numbers(other) == Ordering::Equal
    }

    /// True when this version starts with `prefix` (`16.4` starts with `[16]`).
    pub fn starts_with(&self, prefix: &[u64]) -> bool {
        prefix.iter().enumerate().all(|(i, want)| self.segment(i) == *want)
    }

    fn cmp_numbers(&self, other: &Version) -> Ordering {
        let len = self.segments.len().max(other.segments.len());
        (0..len)
            .map(|i| self.segment(i).cmp(&other.segment(i)))
            .find(|o| *o != Ordering::Equal)
            .unwrap_or(Ordering::Equal)
    }
}

/// Split `rc10` into `["rc", "10"]` so numbers compare as numbers.
fn pre_tokens(pre: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut start = 0;
    let bytes = pre.as_bytes();
    for i in 0..=bytes.len() {
        let boundary = i == bytes.len()
            || !bytes[i].is_ascii_alphanumeric()
            || (i > start && bytes[i].is_ascii_digit() != bytes[i - 1].is_ascii_digit());
        if boundary {
            if i > start {
                tokens.push(&pre[start..i]);
            }
            start = if i < bytes.len() && !bytes[i].is_ascii_alphanumeric() { i + 1 } else { i };
        }
    }
    tokens
}

fn cmp_pre(a: &str, b: &str) -> Ordering {
    let (ta, tb) = (pre_tokens(a), pre_tokens(b));
    for (x, y) in ta.iter().zip(tb.iter()) {
        let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(nx), Ok(ny)) => nx.cmp(&ny),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => x.cmp(y),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    ta.len().cmp(&tb.len())
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.cmp_numbers(other).then_with(|| match (&self.pre, &other.pre) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => cmp_pre(a, b),
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Version {}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let nums: Vec<String> = self.segments.iter().map(u64::to_string).collect();
        write!(f, "{}", nums.join("."))?;
        if let Some(pre) = &self.pre {
            write!(f, "-{pre}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn parses_common_shapes() {
        assert_eq!(v("16").segments(), [16]);
        assert_eq!(v("v1.33.2").segments(), [1, 33, 2]);
        assert_eq!(v("10.0.20348").segments(), [10, 0, 20348]);
        assert_eq!(v(" 3.12 ").segments(), [3, 12]);
        assert_eq!(v("21.0.2+13").to_string(), "21.0.2");
    }

    #[test]
    fn rejects_names_without_numbers() {
        for bad in ["", "subscription", "latest", "v", "1..2", "-1"] {
            assert!(Version::parse(bad).is_err(), "{bad:?} should not parse");
        }
    }

    #[test]
    fn pre_release_markers_are_detected() {
        for pre in ["17.0-rc1", "17.0rc1", "3.12.0b2", "17.0-beta.2", "2.0-SNAPSHOT", "1.0.0-alpha", "5.0-M3"] {
            assert!(v(pre).is_pre_release(), "{pre}");
        }
    }

    #[test]
    fn variant_labels_are_ignored() {
        for (label, plain) in [
            ("16.4-alpine", "16.4"),
            ("2019 LTSC", "2019"),
            ("16-bookworm", "16"),
            ("8u412", "8"),
            ("12.04-precise", "12.04"),
            ("7.4-eap", "7.4"),
            ("2.1-developer", "2.1"),
        ] {
            assert!(!v(label).is_pre_release(), "{label}");
            assert_eq!(v(label), v(plain));
        }
    }

    #[test]
    fn missing_segments_count_as_zero() {
        assert_eq!(v("16"), v("16.0.0"));
        assert!(v("16.0.1") > v("16"));
    }

    #[test]
    fn numbers_compare_numerically() {
        assert!(v("1.10") > v("1.9"));
        assert!(v("10") > v("9.99"));
    }

    #[test]
    fn pre_release_sorts_before_release() {
        assert!(v("17.0-rc1") < v("17.0"));
        assert!(v("17.0-rc1") > v("16.9"));
        assert!(v("17.0-beta2") < v("17.0-rc1"));
        assert!(v("17.0-rc2") < v("17.0-rc10"));
        assert!(v("17.0-rc") < v("17.0-rc1"));
        assert!(v("17.0-rc1").same_release(&v("17")));
    }

    #[test]
    fn prefix_matching() {
        assert!(v("16.4").starts_with(&[16]));
        assert!(v("16").starts_with(&[16, 0]));
        assert!(!v("16.4").starts_with(&[16, 0]));
        assert!(!v("160").starts_with(&[16]));
    }
}
