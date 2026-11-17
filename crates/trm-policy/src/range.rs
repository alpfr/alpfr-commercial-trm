use std::fmt;
use std::str::FromStr;

use crate::version::Version;

/// A version range as stored in `decision.version_range`.
///
/// Grammar:
/// * comparators `>=`, `>`, `<=`, `<`, `=` (or `==`), `!=` followed by a version
/// * a bare version or wildcard is a prefix: `16`, `16.*` and `16.x` all match
///   every `16.y.z`; `*` matches every version
/// * whitespace or a comma means AND: `>=16 <18`, `>=16, <18`
/// * `||` means OR: `>=14 <15 || >=16 <18`
///
/// A pre-release version matches only when the same AND-group names a
/// pre-release of that exact release, so `>=16 <18` does not admit `17.0-rc1`
/// but `>=17.0-rc1 <18` does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range {
    alternatives: Vec<Vec<Comparator>>,
    source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Comparator {
    Any,
    Prefix(Vec<u64>),
    Cmp(Op, Version),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Ge,
    Gt,
    Le,
    Lt,
    Eq,
    Ne,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeError(pub String);

impl fmt::Display for RangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid version range: {}", self.0)
    }
}

impl std::error::Error for RangeError {}

const OPERATORS: &[(&str, Op)] = &[
    (">=", Op::Ge),
    ("<=", Op::Le),
    ("==", Op::Eq),
    ("!=", Op::Ne),
    (">", Op::Gt),
    ("<", Op::Lt),
    ("=", Op::Eq),
];

fn split_operator(token: &str) -> (Option<Op>, &str) {
    for (symbol, op) in OPERATORS {
        if let Some(rest) = token.strip_prefix(symbol) {
            return (Some(*op), rest);
        }
    }
    (None, token)
}

fn parse_comparator(token: &str) -> Result<Comparator, RangeError> {
    let fail = |why: &str| RangeError(format!("{why} in {token:?}"));
    if token.starts_with(['^', '~']) {
        return Err(fail("^ and ~ are not supported; write the bounds out, e.g. >=1.2 <2"));
    }
    let (op, rest) = split_operator(token);
    if rest.is_empty() {
        return Err(fail("missing version"));
    }
    let prefix_ok = matches!(op, None | Some(Op::Eq));

    if matches!(rest, "*" | "x" | "X") {
        return if prefix_ok { Ok(Comparator::Any) } else { Err(fail("wildcard cannot follow this operator")) };
    }
    if let Some(stem) = rest.strip_suffix(".*").or_else(|| rest.strip_suffix(".x")).or_else(|| rest.strip_suffix(".X")) {
        if !prefix_ok {
            return Err(fail("wildcard cannot follow this operator"));
        }
        let version = Version::parse(stem).map_err(|_| fail("bad version"))?;
        if version.is_pre_release() {
            return Err(fail("wildcard cannot follow a pre-release"));
        }
        return Ok(Comparator::Prefix(version.segments().to_vec()));
    }

    let version = Version::parse(rest).map_err(|_| fail("bad version"))?;
    Ok(match op {
        // A bare release number is a prefix: "16" means every 16.y.z.
        None if !version.is_pre_release() => Comparator::Prefix(version.segments().to_vec()),
        None => Comparator::Cmp(Op::Eq, version),
        Some(op) => Comparator::Cmp(op, version),
    })
}

impl Range {
    pub fn parse(input: &str) -> Result<Self, RangeError> {
        let source = input.trim();
        if source.is_empty() {
            return Err(RangeError("empty range".into()));
        }
        let mut alternatives = Vec::new();
        for part in source.split("||") {
            let cleaned = part.replace(',', " ");
            let mut comparators = Vec::new();
            let mut pending_op: Option<&str> = None;
            for token in cleaned.split_whitespace() {
                // Allow a space after the operator: ">= 16".
                let joined;
                let token = match pending_op.take() {
                    Some(op) => {
                        joined = format!("{op}{token}");
                        joined.as_str()
                    }
                    None => token,
                };
                if OPERATORS.iter().any(|(symbol, _)| *symbol == token) {
                    pending_op = OPERATORS.iter().map(|(s, _)| *s).find(|s| *s == token);
                    continue;
                }
                comparators.push(parse_comparator(token)?);
            }
            if pending_op.is_some() {
                return Err(RangeError(format!("operator without a version in {part:?}")));
            }
            if comparators.is_empty() {
                return Err(RangeError(format!("empty alternative in {source:?}")));
            }
            alternatives.push(comparators);
        }
        Ok(Range { alternatives, source: source.to_owned() })
    }

    pub fn matches(&self, version: &Version) -> bool {
        self.alternatives.iter().any(|group| {
            group.iter().all(|c| c.matches(version)) && (!version.is_pre_release() || admits_pre_release(group, version))
        })
    }

    /// Parse `version` and match it; an unparseable version never matches.
    pub fn matches_str(&self, version: &str) -> bool {
        Version::parse(version).is_ok_and(|v| self.matches(&v))
    }

    pub fn as_str(&self) -> &str {
        &self.source
    }
}

fn admits_pre_release(group: &[Comparator], version: &Version) -> bool {
    group.iter().any(|c| matches!(c, Comparator::Cmp(_, bound) if bound.is_pre_release() && bound.same_release(version)))
}

impl Comparator {
    fn matches(&self, version: &Version) -> bool {
        match self {
            Comparator::Any => true,
            Comparator::Prefix(prefix) => version.starts_with(prefix),
            Comparator::Cmp(op, bound) => match op {
                Op::Ge => version >= bound,
                Op::Gt => version > bound,
                Op::Le => version <= bound,
                Op::Lt => version < bound,
                Op::Eq => version == bound,
                Op::Ne => version != bound,
            },
        }
    }
}

impl FromStr for Range {
    type Err = RangeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Range::parse(s)
    }
}

impl fmt::Display for Range {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(range: &str, yes: &[&str], no: &[&str]) {
        let r = Range::parse(range).unwrap_or_else(|e| panic!("{range}: {e}"));
        for v in yes {
            assert!(r.matches_str(v), "{range} should match {v}");
        }
        for v in no {
            assert!(!r.matches_str(v), "{range} should not match {v}");
        }
    }

    #[test]
    fn bounded_range() {
        check(">=16 <18", &["16", "16.0", "16.4", "17.9.9", "v17.2"], &["15.9", "18", "18.0.1", "9"]);
        check(">=16, <18", &["16.4"], &["18"]);
        check(">= 16 < 18", &["16.4"], &["18"]);
        check(">16 <=18", &["16.0.1", "18", "18.0"], &["16", "18.0.1"]);
    }

    #[test]
    fn bare_version_and_wildcards_are_prefixes() {
        check("16", &["16", "16.4", "16.10.2"], &["17", "1.6", "160"]);
        check("16.*", &["16.0", "16.4"], &["17.0"]);
        check("1.33.x", &["1.33", "1.33.7"], &["1.34.0", "1.3"]);
        check("*", &["0.1", "99"], &["latest"]);
    }

    #[test]
    fn exact_and_not_equal() {
        check("=16.4", &["16.4", "16.4.0"], &["16.4.1", "16"]);
        check("==16.4", &["16.4"], &["16.5"]);
        check(">=16 <18 !=16.3", &["16.2", "16.4"], &["16.3", "16.3.0"]);
    }

    #[test]
    fn alternatives() {
        check(">=14 <15 || >=16 <18", &["14.2", "16.4", "17"], &["15.1", "13", "18"]);
        check("12 || 14", &["12.9", "14.0"], &["13.0"]);
    }

    #[test]
    fn pre_releases_need_an_explicit_bound() {
        check(">=16 <18", &["17.0"], &["17.0-rc1", "18.0-beta1", "16.0-rc1"]);
        check(">=17.0-rc1 <18", &["17.0-rc1", "17.0-rc2", "17.0", "17.5"], &["17.0-beta1", "17.1-rc1", "18.0-rc1"]);
        check("17.0-rc1", &["17.0-rc1"], &["17.0", "17.0-rc2"]);
        check("*", &["17.0"], &["17.0-rc1"]);
    }

    #[test]
    fn variant_labels_match_like_the_plain_version() {
        check(">=16 <18", &["16.4-alpine", "17-bookworm"], &["18-alpine"]);
    }

    #[test]
    fn unparseable_versions_never_match() {
        check("*", &[], &["subscription", ""]);
    }

    #[test]
    fn invalid_ranges_are_rejected() {
        for bad in ["", "  ", ">=", ">=16 <", "^1.2", "~1.2", ">=abc", ">*", "<16.*", ">=16 ||", "|| 16", "!=16.*", "17.0-rc1.*"] {
            assert!(Range::parse(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn keeps_source_text() {
        assert_eq!(Range::parse("  >=16 <18 ").unwrap().to_string(), ">=16 <18");
    }
}
