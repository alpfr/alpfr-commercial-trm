use std::cmp::Reverse;
use std::str::FromStr;

use chrono::NaiveDate;

use crate::range::Range;
use crate::version::Version;

/// Mirrors the `decision_status` enum in `db/schema.sql`.
/// Ordered from least to most restrictive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Approved,
    ApprovedWithConstraints,
    Divest,
    Prohibited,
}

impl FromStr for Status {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "approved" => Ok(Status::Approved),
            "approved_with_constraints" => Ok(Status::ApprovedWithConstraints),
            "divest" => Ok(Status::Divest),
            "prohibited" => Ok(Status::Prohibited),
            other => Err(format!("unknown decision status: {other}")),
        }
    }
}

/// What a decision applies to within one technology.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Every version (`technology_version_id` and `version_range` both NULL).
    All,
    /// One release line from the catalog, e.g. `16` (`technology_version_id`).
    Line(String),
    /// A range expression (`version_range`).
    Range(Range),
}

impl Target {
    fn matches(&self, raw: &str, parsed: Option<&Version>) -> bool {
        match self {
            Target::All => true,
            Target::Range(range) => parsed.is_some_and(|v| range.matches(v)),
            Target::Line(line) => match (Version::parse(line), parsed) {
                (Ok(line_v), Some(v)) if !v.is_pre_release() && !line_v.is_pre_release() => {
                    v.starts_with(line_v.segments())
                }
                // Named lines ("subscription") and pre-releases match by exact text only.
                _ => line.trim().eq_ignore_ascii_case(raw.trim()),
            },
        }
    }

    fn specificity(&self) -> u8 {
        match self {
            Target::Line(_) => 2,
            Target::Range(_) => 1,
            Target::All => 0,
        }
    }
}

/// One current (not superseded) decision for a technology.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub id: String,
    pub target: Target,
    pub status: Status,
    pub constraints: Option<String>,
    /// `None` applies to every environment.
    pub environment: Option<String>,
    /// `None` applies to every data classification.
    pub data_classification: Option<String>,
    pub valid_from: NaiveDate,
    pub valid_to: NaiveDate,
}

/// What an enforcement point is asking about.
#[derive(Debug, Clone, Copy)]
pub struct Query<'a> {
    pub version: &'a str,
    pub environment: Option<&'a str>,
    pub data_classification: Option<&'a str>,
    /// The date to evaluate on, normally today.
    pub on: NaiveDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Approved.
    Allow,
    /// Approved with constraints; the caller must surface `constraints`.
    AllowWithConstraints,
    /// Divest: in use today, no new adoption.
    Warn,
    /// Prohibited.
    Deny,
    /// The only matching decisions have expired; tenant policy decides.
    Expired,
    /// No decision covers this version and scope; tenant policy decides.
    Unapproved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome<'a> {
    pub verdict: Verdict,
    /// The decision the verdict came from, if any.
    pub decision: Option<&'a Decision>,
}

fn scope_field_applies(decision: Option<&str>, query: Option<&str>) -> bool {
    match (decision, query) {
        (None, _) => true,
        // A scoped decision applies only when the caller states that scope.
        (Some(d), Some(q)) => d.trim().eq_ignore_ascii_case(q.trim()),
        (Some(_), None) => false,
    }
}

impl Decision {
    fn scope_specificity(&self) -> u8 {
        u8::from(self.environment.is_some()) + u8::from(self.data_classification.is_some())
    }

    fn applies_to(&self, query: &Query, parsed: Option<&Version>) -> bool {
        self.valid_from <= query.on
            && scope_field_applies(self.environment.as_deref(), query.environment)
            && scope_field_applies(self.data_classification.as_deref(), query.data_classification)
            && self.target.matches(query.version, parsed)
    }
}

/// Pick the governing decision among `decisions` (one technology, one tenant,
/// current rows only) and return the verdict.
///
/// Order of precedence among decisions that apply:
/// 1. decisions in force beat expired ones
/// 2. more specific target: release line, then range, then all versions
/// 3. more specific scope: more of environment and data classification set
/// 4. more restrictive status
/// 5. most recent `valid_from`
pub fn evaluate<'a>(decisions: &'a [Decision], query: &Query) -> Outcome<'a> {
    let parsed = Version::parse(query.version).ok();
    let best = decisions
        .iter()
        .filter(|d| d.applies_to(query, parsed.as_ref()))
        .min_by_key(|d| {
            (
                d.valid_to < query.on,
                Reverse(d.target.specificity()),
                Reverse(d.scope_specificity()),
                Reverse(d.status),
                Reverse(d.valid_from),
            )
        });

    let verdict = match best {
        None => Verdict::Unapproved,
        Some(d) if d.valid_to < query.on => Verdict::Expired,
        Some(d) => match d.status {
            Status::Approved => Verdict::Allow,
            Status::ApprovedWithConstraints => Verdict::AllowWithConstraints,
            Status::Divest => Verdict::Warn,
            Status::Prohibited => Verdict::Deny,
        },
    };
    Outcome { verdict, decision: best }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn decision(id: &str, target: Target, status: Status) -> Decision {
        Decision {
            id: id.into(),
            target,
            status,
            constraints: None,
            environment: None,
            data_classification: None,
            valid_from: day(2026, 1, 1),
            valid_to: day(2026, 12, 31),
        }
    }

    fn range(s: &str) -> Target {
        Target::Range(Range::parse(s).unwrap())
    }

    fn line(s: &str) -> Target {
        Target::Line(s.into())
    }

    fn ask<'a>(decisions: &'a [Decision], version: &str) -> (Verdict, Option<&'a str>) {
        ask_scoped(decisions, version, None, None, day(2026, 6, 1))
    }

    fn ask_scoped<'a>(
        decisions: &'a [Decision],
        version: &str,
        environment: Option<&str>,
        data_classification: Option<&str>,
        on: NaiveDate,
    ) -> (Verdict, Option<&'a str>) {
        let out = evaluate(decisions, &Query { version, environment, data_classification, on });
        (out.verdict, out.decision.map(|d| d.id.as_str()))
    }

    #[test]
    fn no_decision_is_unapproved() {
        assert_eq!(ask(&[], "16.4"), (Verdict::Unapproved, None));
        let ds = [decision("a", range(">=16 <18"), Status::Approved)];
        assert_eq!(ask(&ds, "15.2"), (Verdict::Unapproved, None));
    }

    #[test]
    fn status_maps_to_verdict() {
        for (status, verdict) in [
            (Status::Approved, Verdict::Allow),
            (Status::ApprovedWithConstraints, Verdict::AllowWithConstraints),
            (Status::Divest, Verdict::Warn),
            (Status::Prohibited, Verdict::Deny),
        ] {
            let ds = [decision("a", Target::All, status)];
            assert_eq!(ask(&ds, "1.0").0, verdict);
        }
    }

    #[test]
    fn release_line_beats_range_beats_all() {
        let ds = [
            decision("all", Target::All, Status::Prohibited),
            decision("range", range(">=12 <18"), Status::Approved),
            decision("line", line("12"), Status::Divest),
        ];
        assert_eq!(ask(&ds, "12.22"), (Verdict::Warn, Some("line")));
        assert_eq!(ask(&ds, "16.4"), (Verdict::Allow, Some("range")));
        assert_eq!(ask(&ds, "9.6"), (Verdict::Deny, Some("all")));
    }

    #[test]
    fn release_line_matches_by_prefix_or_exact_name() {
        let ds = [
            decision("num", line("1.33"), Status::Approved),
            decision("named", line("Subscription"), Status::Approved),
        ];
        assert_eq!(ask(&ds, "1.33.4").1, Some("num"));
        assert_eq!(ask(&ds, "1.3").1, None);
        assert_eq!(ask(&ds, "subscription").1, Some("named"));
        assert_eq!(ask(&ds, "1.33.0-rc1").1, None);
    }

    #[test]
    fn more_restrictive_status_wins_a_tie() {
        let ds = [
            decision("ok", range(">=16 <18"), Status::Approved),
            decision("no", range("16.3"), Status::Prohibited),
        ];
        assert_eq!(ask(&ds, "16.3.1"), (Verdict::Deny, Some("no")));
        assert_eq!(ask(&ds, "16.4"), (Verdict::Allow, Some("ok")));
    }

    #[test]
    fn scoped_decision_needs_a_matching_scope() {
        let mut prod = decision("prod", range(">=16 <18"), Status::Prohibited);
        prod.environment = Some("production".into());
        let ds = [decision("any", range(">=16 <18"), Status::Approved), prod];
        let on = day(2026, 6, 1);
        assert_eq!(ask_scoped(&ds, "16.4", Some("Production"), None, on), (Verdict::Deny, Some("prod")));
        assert_eq!(ask_scoped(&ds, "16.4", Some("dev"), None, on), (Verdict::Allow, Some("any")));
        assert_eq!(ask_scoped(&ds, "16.4", None, None, on), (Verdict::Allow, Some("any")));
    }

    #[test]
    fn target_specificity_outranks_scope_specificity() {
        let mut scoped_all = decision("scoped-all", Target::All, Status::Prohibited);
        scoped_all.environment = Some("production".into());
        scoped_all.data_classification = Some("phi".into());
        let ds = [scoped_all, decision("line", line("16"), Status::Approved)];
        let got = ask_scoped(&ds, "16.4", Some("production"), Some("phi"), day(2026, 6, 1));
        assert_eq!(got, (Verdict::Allow, Some("line")));
    }

    #[test]
    fn expired_decisions_yield_to_ones_in_force() {
        let mut old = decision("old-line", line("16"), Status::Approved);
        old.valid_to = day(2026, 3, 31);
        let ds = [old, decision("range", range(">=16 <18"), Status::Divest)];
        assert_eq!(ask(&ds, "16.4"), (Verdict::Warn, Some("range")));
    }

    #[test]
    fn only_expired_matches_report_expired() {
        let mut old = decision("old", range(">=16 <18"), Status::Approved);
        old.valid_to = day(2026, 3, 31);
        let ds = [old];
        assert_eq!(ask(&ds, "16.4"), (Verdict::Expired, Some("old")));
        // valid_to is inclusive
        assert_eq!(ask_scoped(&ds, "16.4", None, None, day(2026, 3, 31)).0, Verdict::Allow);
    }

    #[test]
    fn future_decisions_are_ignored() {
        let mut later = decision("later", Target::All, Status::Prohibited);
        later.valid_from = day(2026, 9, 1);
        assert_eq!(ask(&[later], "1.0"), (Verdict::Unapproved, None));
    }

    #[test]
    fn newest_wins_when_everything_else_ties() {
        let mut newer = decision("newer", range(">=16 <18"), Status::Approved);
        newer.valid_from = day(2026, 5, 1);
        let ds = [decision("older", range(">=16 <19"), Status::Approved), newer];
        assert_eq!(ask(&ds, "16.4").1, Some("newer"));
    }

    #[test]
    fn unparseable_version_can_match_all_or_a_named_line_only() {
        let ds = [
            decision("range", range("*"), Status::Approved),
            decision("all", Target::All, Status::Divest),
        ];
        assert_eq!(ask(&ds, "latest"), (Verdict::Warn, Some("all")));
    }

    #[test]
    fn status_parses_from_database_text() {
        assert_eq!("approved_with_constraints".parse(), Ok(Status::ApprovedWithConstraints));
        assert!("unapproved".parse::<Status>().is_err());
    }
}
