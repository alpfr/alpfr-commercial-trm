"""Check versions against a range from the command line.

    python -m trm_policy '>=16 <18' 16.4 18.0 17.0-rc1

Exit code: 0 when every version matches, 1 when any does not,
2 when the range is invalid or the arguments are wrong.
"""
import sys

from . import Range, RangeError


def main(argv: list[str]) -> int:
    if len(argv) < 2 or argv[0] in ("-h", "--help"):
        print("usage: python -m trm_policy <RANGE> <VERSION>...\n"
              "  e.g. python -m trm_policy '>=16 <18' 16.4 18.0", file=sys.stderr)
        return 2
    try:
        rng = Range.parse(argv[0])
    except RangeError as err:
        print(f"error: {err}", file=sys.stderr)
        return 2
    all_match = True
    for version in argv[1:]:
        matched = rng.matches(version)
        all_match &= matched
        print(f"{version}\t{'match' if matched else 'no match'}")
    return 0 if all_match else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
