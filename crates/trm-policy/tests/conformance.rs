//! Runs the shared cases in tests/vectors/policy.json. The Python package
//! `trm_policy` runs the same file (tests/test_policy.py), which is what keeps
//! the two implementations in step.

use chrono::NaiveDate;
use serde_json::Value;
use trm_policy::{evaluate, Decision, Query, Range, Status, Target, Verdict, Version};

fn vectors() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/vectors/policy.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("vectors file")).expect("vectors JSON")
}

fn list(v: &Value) -> &Vec<Value> {
    v.as_array().expect("array")
}

fn s(v: &Value) -> &str {
    v.as_str().expect("string")
}

fn date(v: &Value, fallback: &Value) -> NaiveDate {
    let text = v.as_str().unwrap_or_else(|| s(fallback));
    NaiveDate::parse_from_str(text, "%Y-%m-%d").expect("date")
}

#[test]
fn versions() {
    let all = vectors();
    for case in list(&all["versions"]["valid"]) {
        let input = s(&case["input"]);
        let version = Version::parse(input).unwrap_or_else(|e| panic!("{input:?}: {e}"));
        assert_eq!(version.to_string(), s(&case["display"]), "display of {input:?}");
        assert_eq!(version.is_pre_release(), case["pre"].as_bool().unwrap(), "pre flag of {input:?}");
    }
    for bad in list(&all["versions"]["invalid"]) {
        assert!(Version::parse(s(bad)).is_err(), "{bad} should not parse");
    }
}

#[test]
fn ordering() {
    for case in list(&vectors()["ordering"]) {
        let (left, relation, right) = (s(&case[0]), s(&case[1]), s(&case[2]));
        let (a, b) = (Version::parse(left).unwrap(), Version::parse(right).unwrap());
        match relation {
            "=" => assert!(a == b && b == a && a <= b && a >= b, "{left} = {right}"),
            "<" => assert!(a < b && b > a && a != b, "{left} < {right}"),
            other => panic!("unknown relation {other}"),
        }
    }
}

#[test]
fn ranges() {
    let all = vectors();
    for case in list(&all["ranges"]) {
        let text = s(&case["range"]);
        let range = Range::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"));
        for v in list(&case["match"]) {
            assert!(range.matches_str(s(v)), "{text} should match {v}");
        }
        for v in list(&case["no_match"]) {
            assert!(!range.matches_str(s(v)), "{text} should not match {v}");
        }
    }
    for bad in list(&all["invalid_ranges"]) {
        assert!(Range::parse(s(bad)).is_err(), "{bad} should be rejected");
    }
}

fn verdict_name(v: Verdict) -> &'static str {
    match v {
        Verdict::Allow => "allow",
        Verdict::AllowWithConstraints => "allow_with_constraints",
        Verdict::Warn => "warn",
        Verdict::Deny => "deny",
        Verdict::Expired => "expired",
        Verdict::Unapproved => "unapproved",
    }
}

#[test]
fn decisions() {
    let all = vectors();
    let defaults = &all["decision_defaults"];
    for case in list(&all["decisions"]) {
        let name = s(&case["name"]);
        let decisions: Vec<Decision> = list(&case["decisions"])
            .iter()
            .map(|d| Decision {
                id: s(&d["id"]).to_owned(),
                target: match &d["target"] {
                    Value::String(all) if all == "all" => Target::All,
                    t if t["line"].is_string() => Target::Line(s(&t["line"]).to_owned()),
                    t => Target::Range(Range::parse(s(&t["range"])).unwrap()),
                },
                status: s(&d["status"]).parse::<Status>().unwrap(),
                constraints: d["constraints"].as_str().map(str::to_owned),
                environment: d["environment"].as_str().map(str::to_owned),
                data_classification: d["data_classification"].as_str().map(str::to_owned),
                valid_from: date(&d["valid_from"], &defaults["valid_from"]),
                valid_to: date(&d["valid_to"], &defaults["valid_to"]),
            })
            .collect();
        for q in list(&case["queries"]) {
            let query = Query {
                version: s(&q["version"]),
                environment: q["environment"].as_str(),
                data_classification: q["data_classification"].as_str(),
                on: date(&q["on"], &defaults["on"]),
            };
            let outcome = evaluate(&decisions, &query);
            let label = format!("{name}: {q}");
            assert_eq!(verdict_name(outcome.verdict), s(&q["verdict"]), "verdict for {label}");
            assert_eq!(outcome.decision.map(|d| d.id.as_str()), q["decision"].as_str(), "decision for {label}");
        }
    }
}
