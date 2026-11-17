"""Policy core tests. The cases come from tests/vectors/policy.json, which the
Rust crate runs too (crates/trm-policy/tests/conformance.rs)."""
import json
import pathlib
import subprocess
import sys
from datetime import date

import pytest

from trm_policy import (Decision, Query, Range, RangeError, Status, Verdict,
                        Version, VersionError, evaluate)

ROOT = pathlib.Path(__file__).resolve().parents[1]
VECTORS = json.loads((ROOT / "tests" / "vectors" / "policy.json").read_text(encoding="utf-8"))
DEFAULTS = VECTORS["decision_defaults"]


@pytest.mark.parametrize("case", VECTORS["versions"]["valid"], ids=lambda c: repr(c["input"]))
def test_valid_version(case):
    version = Version.parse(case["input"])
    assert str(version) == case["display"]
    assert version.is_pre_release is case["pre"]


@pytest.mark.parametrize("text", VECTORS["versions"]["invalid"], ids=repr)
def test_invalid_version(text):
    with pytest.raises(VersionError):
        Version.parse(text)


@pytest.mark.parametrize("left,relation,right", VECTORS["ordering"])
def test_ordering(left, relation, right):
    a, b = Version.parse(left), Version.parse(right)
    if relation == "=":
        assert a == b and b == a and not a < b and not b < a and a <= b and a >= b
    else:
        assert a < b and b > a and a != b and not b < a and a <= b


@pytest.mark.parametrize("case", VECTORS["ranges"], ids=lambda c: c["range"])
def test_range(case):
    rng = Range.parse(case["range"])
    assert [v for v in case["match"] if not rng.matches(v)] == [], "should match"
    assert [v for v in case["no_match"] if rng.matches(v)] == [], "should not match"


@pytest.mark.parametrize("text", VECTORS["invalid_ranges"], ids=repr)
def test_invalid_range(text):
    with pytest.raises(RangeError):
        Range.parse(text)


def build_decision(raw: dict) -> Decision:
    target = raw["target"]
    return Decision(
        id=raw["id"],
        status=Status.from_db(raw["status"]),
        valid_from=date.fromisoformat(raw.get("valid_from", DEFAULTS["valid_from"])),
        valid_to=date.fromisoformat(raw.get("valid_to", DEFAULTS["valid_to"])),
        line=None if target == "all" else target.get("line"),
        range=Range.parse(target["range"]) if target != "all" and "range" in target else None,
        constraints=raw.get("constraints"),
        environment=raw.get("environment"),
        data_classification=raw.get("data_classification"),
    )


DECISION_QUERIES = [
    pytest.param(case, query, id=f'{case["name"]}: {query["version"]}#{i}')
    for case in VECTORS["decisions"]
    for i, query in enumerate(case["queries"])
]


@pytest.mark.parametrize("case,query", DECISION_QUERIES)
def test_decision(case, query):
    decisions = [build_decision(d) for d in case["decisions"]]
    outcome = evaluate(decisions, Query(
        version=query["version"],
        on=date.fromisoformat(query.get("on", DEFAULTS["on"])),
        environment=query.get("environment"),
        data_classification=query.get("data_classification"),
    ))
    assert outcome.verdict == Verdict(query["verdict"])
    assert (outcome.decision.id if outcome.decision else None) == query["decision"]


def test_no_decisions_at_all():
    assert evaluate([], Query("1.0", date(2026, 6, 1))) == (
        __import__("trm_policy").Outcome(Verdict.UNAPPROVED, None))


def test_status_from_database_text():
    assert Status.from_db("approved_with_constraints") is Status.APPROVED_WITH_CONSTRAINTS
    for bad in ("unapproved", "APPROVED", ""):
        with pytest.raises(ValueError):
            Status.from_db(bad)


def test_decision_cannot_target_line_and_range():
    with pytest.raises(ValueError):
        Decision("x", Status.APPROVED, date(2026, 1, 1), date(2026, 12, 31),
                 line="16", range=Range.parse("16"))


def test_command_line_exit_codes():
    def run(*args):
        return subprocess.run([sys.executable, "-m", "trm_policy", *args],
                              cwd=ROOT, capture_output=True, text=True)
    ok = run(">=16 <18", "16.4", "17.0")
    assert ok.returncode == 0 and ok.stdout == "16.4\tmatch\n17.0\tmatch\n"
    assert run(">=16 <18", "16.4", "18.0").returncode == 1
    assert run("^1.2", "1.3").returncode == 2
    assert run().returncode == 2
