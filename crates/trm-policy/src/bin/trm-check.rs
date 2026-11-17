//! Check versions against a range from the command line.
//!
//!     trm-check '>=16 <18' 16.4 18.0 17.0-rc1
//!
//! Exit code: 0 when every version matches, 1 when any does not,
//! 2 when the range is invalid or the arguments are wrong.

use std::process::ExitCode;

use trm_policy::Range;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 || args[0] == "-h" || args[0] == "--help" {
        eprintln!("usage: trm-check <RANGE> <VERSION>...\n  e.g. trm-check '>=16 <18' 16.4 18.0");
        return ExitCode::from(2);
    }
    let range = match Range::parse(&args[0]) {
        Ok(range) => range,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(2);
        }
    };
    let mut all_match = true;
    for version in &args[1..] {
        let matched = range.matches_str(version);
        all_match &= matched;
        println!("{version}\t{}", if matched { "match" } else { "no match" });
    }
    if all_match { ExitCode::SUCCESS } else { ExitCode::from(1) }
}
