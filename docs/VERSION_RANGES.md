# Version ranges and decision evaluation

These are the rules implemented by `crates/trm-policy` (Rust) and `trm_policy/`
(Python). The shared cases in `tests/vectors/policy.json` are the executable
form of this document, and both implementations must pass all of them.

## Versions

Versions are parsed leniently, because discovery reports them in many shapes.

| Input | Read as | Note |
| --- | --- | --- |
| `16`, `16.0`, `16.0.0` | 16.0.0 | Missing segments count as zero |
| `12.04` | 12.4 | Leading zeros in a segment are dropped |
| `v1.33.2` | 1.33.2 | Leading `v` is dropped |
| `21.0.2+13` | 21.0.2 | Build metadata is ignored |
| `16.4-alpine`, `2019 LTSC`, `8u412` | 16.4, 2019, 8 | Variant labels are ignored |
| `17.0-rc1`, `3.12.0b2`, `2.0-SNAPSHOT` | pre-release | Sorts before the release with the same numbers |
| `subscription`, `latest` | not a version | Never matches a range |

Pre-release markers: `alpha`, `beta`, `rc`, `pre`, `preview`, `dev`, `snapshot`,
`milestone`, `nightly`, `canary`, `ea`, and the shorthands `a1`, `b2`, `m3`. A
marker counts only when it stands alone or is followed by a number or separator,
so `precise` and `eap` are variant labels. Pre-releases of the same release are
ordered by comparing their text, with numbers compared as numbers (`rc2` before
`rc10`).

## Range grammar

| Form | Meaning | Example |
| --- | --- | --- |
| `>=V`, `>V`, `<=V`, `<V` | Bound | `>=16 <18` |
| `=V` or `==V` | Exactly that version | `=16.4` matches 16.4 and 16.4.0, not 16.4.1 |
| `!=V` | Not that version | `>=16 <18 !=16.3` |
| `V` (bare), `V.*`, `V.x` | Prefix: every version starting with V | `16` matches 16.0 and 16.10.2 |
| `*` | Every version | |
| space or comma | AND | `>=16, <18` |
| `\|\|` | OR | `>=14 <15 \|\| >=16 <18` |

`^` and `~` are rejected on purpose: their meaning differs between ecosystems.
Write the bounds out instead.

**Pre-release rule.** A pre-release matches only when the same AND-group has a
bound that names a pre-release of that exact release. `>=16 <18` does not admit
`17.0-rc1` or `18.0-beta1`; `>=17.0-rc1 <18` admits `17.0-rc1` and `17.0-rc2`.

An empty range is invalid. To target every version, leave both
`technology_version_id` and `version_range` NULL.

## What a decision targets

| `technology_version_id` | `version_range` | Target | Matches |
| --- | --- | --- | --- |
| set | NULL | Release line | Versions starting with the line (`16` covers 16.4). A named line such as `subscription` matches by exact text |
| NULL | set | Range | Versions the range admits |
| NULL | NULL | All | Every version, including unparseable ones |

## Scope

A decision with `environment` or `data_classification` set applies only when
the query states the same value (compared without regard to case). A query with
no environment never matches an environment-scoped decision, so enforcement
points should always send their scope.

## Order of precedence

Among the current decisions that apply to a query on a given date:

1. A decision in force beats an expired one. `valid_to` is inclusive, and a
   decision whose `valid_from` is in the future is ignored.
2. The more specific target wins: release line, then range, then all.
3. The more specific scope wins: more of environment and data classification set.
4. The more restrictive status wins: prohibited, divest, approved with
   constraints, approved.
5. The most recent `valid_from` wins.

## Verdicts

| Governing decision | Verdict | Enforcement point should |
| --- | --- | --- |
| approved | `Allow` | Proceed |
| approved_with_constraints | `AllowWithConstraints` | Proceed and surface the constraints |
| divest | `Warn` | Allow existing use, block new adoption per tenant policy |
| prohibited | `Deny` | Block |
| only expired decisions apply | `Expired` | Follow tenant policy (warn or deny) |
| nothing applies | `Unapproved` | Follow tenant policy (warn or deny) |

## Not handled yet

- Ecosystem-specific schemes (Debian epochs, calendar versions with letters).
- Validation at write time: the database stores `version_range` as text, so the
  API must parse it (`Range::parse` in Rust, `Range.parse` in Python) before
  inserting a decision.
- Discovery should still normalise image tags before asking; only simple
  variant labels are ignored here.
