//! Measure how smooth saved clips are: repeated pictures (short freezes)
//! and holes in the timeline (dropped frames).
//!
//!   cargo run --release --example judder -- <clip.mp4>...
//!
//! One line per clip, then its worst moments. The score is the share of
//! the timeline that was a repeated or missing frame; the budget is under
//! 0.5% (docs/DESIGN.md §5).

use std::path::PathBuf;

use clip_core::pacing::{Event, Report, analyze};

fn timecode(ns: u64) -> String {
    let s = ns as f64 / 1e9;
    format!("{}:{:06.3}", (s / 60.0) as u64, s % 60.0)
}

fn print(path: &std::path::Path, r: &Report) {
    let t = &r.totals;
    println!(
        "{}\n  {:.2}% judder  ({} frames at {:.2} fps; {} repeated in {} freezes, longest {}; \
         {} missing in {} holes; {} still, {} pauses)",
        path.display(),
        t.judder_percent(),
        t.frames,
        r.fps,
        t.repeated,
        t.repeat_runs,
        t.longest_repeat,
        t.missing,
        t.holes,
        t.still,
        t.pauses,
    );
    if r.unclear > 0 || r.clear_motion_percent < 50.0 {
        println!(
            "  {:.0}% of the clip had motion clear enough to judge; {} unchanged-looking frames \
             in slow scenes were not counted",
            r.clear_motion_percent, r.unclear,
        );
    }
    if let Some(p) = &r.packets {
        println!(
            "  recorder's guess from packet sizes: {:.2}%  ({} repeated in {} freezes; \
             {} missing in {} holes; {} still)",
            p.judder_percent(),
            p.repeated,
            p.repeat_runs,
            p.missing,
            p.holes,
            p.still,
        );
    }
    // Longest first; the timeline order is in the counts above.
    let mut worst = r.events.clone();
    worst.sort_by_key(|e| {
        std::cmp::Reverse(match *e {
            Event::Hole { missing, .. } => missing,
            Event::Repeat { frames, .. } => frames,
        })
    });
    for e in worst.iter().take(12) {
        match *e {
            Event::Hole { at_ns, missing } => {
                println!("    {}  hole, {missing} missing", timecode(at_ns));
            }
            Event::Repeat { at_ns, frames } => {
                println!("    {}  freeze, {frames} repeated", timecode(at_ns));
            }
        }
    }
    if worst.len() > 12 {
        println!("    … and {} more", worst.len() - 12);
    }
}

/// Packet sizes, as a percentage of the clip's median, for repeated and
/// for new pictures: the 5th, 25th, 50th, 75th and 95th percentile of each.
fn print_sizes(shares: &[(f64, bool)]) {
    for (label, repeat) in [("repeats", true), ("new pictures", false)] {
        let mut v: Vec<f64> = shares
            .iter()
            .filter(|s| s.1 == repeat)
            .map(|s| s.0 * 100.0)
            .collect();
        if v.is_empty() {
            continue;
        }
        v.sort_by(f64::total_cmp);
        let at = |p: usize| v[(v.len() - 1) * p / 100];
        println!(
            "  {label:>12}: {:>6} packets, % of median size p5 {:.1}  p25 {:.1}  p50 {:.1}  p75 {:.1}  p95 {:.1}",
            v.len(),
            at(5),
            at(25),
            at(50),
            at(75),
            at(95),
        );
    }
}

fn main() -> anyhow::Result<()> {
    clip_core::init()?;
    let paths: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if paths.is_empty() {
        anyhow::bail!("usage: judder <clip.mp4>...");
    }
    // RERUNS_JUDDER_SIZES=1: also show how big repeated and new pictures
    // are once encoded, for tuning the recorder's size-only guess.
    let sizes = std::env::var_os("RERUNS_JUDDER_SIZES").is_some();
    let mut failed = false;
    for path in &paths {
        match analyze(path) {
            Ok(report) => {
                print(path, &report);
                if sizes {
                    match clip_core::pacing::size_shares(path) {
                        Ok(shares) => print_sizes(&shares),
                        Err(e) => eprintln!("  sizes: {e:#}"),
                    }
                }
            }
            Err(e) => {
                failed = true;
                eprintln!("{}: {e:#}", path.display());
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
    Ok(())
}
