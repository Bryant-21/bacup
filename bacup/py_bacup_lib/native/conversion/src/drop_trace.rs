//! Env-gated drop tracing for the conversion pipeline.
//!
//! When a subrecord, field, reference, or whole record is dropped/nulled at an
//! instrumented choke point, this records one line so a regen run can show
//! *exactly* where data is lost — instead of inferring it from the diff between
//! the source and converted record.
//!
//! Output goes to a **log file**, never the console, and the hot path is
//! **non-blocking**: each trace `send`s its line over a channel to a single
//! background writer thread that owns the file. The parallel conversion workers
//! never contend on a stderr/file lock. Zero overhead when disabled (one cached
//! `OnceLock` read returns `None` and every call returns early).
//!
//! Driven by env vars (set for you by `regen.py --drop-trace`):
//!   * `MODBOX_TRACE_DROPS`      — filter (required to enable). `1`/`all`, or
//!     comma-separated `SIG` / `SIG:SUB` rules, e.g. `FACT:VENC` or `FACT,QUST`.
//!   * `MODBOX_TRACE_DROPS_FILE` — output path (default `drop_trace.log` in cwd).
//!
//! Line format:
//! ```text
//! [drop_trace] stage=<stage> rec=<SIG>:<local-hex> sub=<SUB> reason=<text>
//! ```

use crossbeam_channel::{Sender, unbounded};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::OnceLock;

enum Filter {
    All,
    /// `(record_sig_uppercase, optional sub_sig_uppercase)` rules; a drop matches
    /// if any rule matches.
    Rules(Vec<(String, Option<String>)>),
}

struct DropTrace {
    filter: Filter,
    sender: Sender<String>,
}

fn parse_filter(raw: &str) -> Option<Filter> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if raw == "1" || raw.eq_ignore_ascii_case("all") {
        return Some(Filter::All);
    }
    let rules: Vec<(String, Option<String>)> = raw
        .split(',')
        .filter_map(|tok| {
            let tok = tok.trim();
            if tok.is_empty() {
                return None;
            }
            Some(match tok.split_once(':') {
                Some((rec, sub)) => (
                    rec.trim().to_ascii_uppercase(),
                    Some(sub.trim().to_ascii_uppercase()),
                ),
                None => (tok.to_ascii_uppercase(), None),
            })
        })
        .collect();
    if rules.is_empty() {
        None
    } else {
        Some(Filter::Rules(rules))
    }
}

fn state() -> Option<&'static DropTrace> {
    static S: OnceLock<Option<DropTrace>> = OnceLock::new();
    S.get_or_init(|| {
        let filter = parse_filter(&std::env::var("MODBOX_TRACE_DROPS").ok()?)?;
        let path = std::env::var("MODBOX_TRACE_DROPS_FILE")
            .unwrap_or_else(|_| "drop_trace.log".to_string());
        let file = match File::create(&path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("[drop_trace] cannot open {path}: {e}; drop tracing disabled");
                return None;
            }
        };

        // Detach the writer for the process lifetime (the JoinHandle is only
        // needed by tests, which drive `start_writer` directly).
        let (sender, _handle) = start_writer(file).ok()?;
        Some(DropTrace { filter, sender })
    })
    .as_ref()
}

/// Spawn the background writer that owns `file` and drains lines until every
/// `Sender` is dropped. Returns the send side plus the join handle (tests join
/// it; production detaches it).
fn start_writer(file: File) -> std::io::Result<(Sender<String>, std::thread::JoinHandle<()>)> {
    let (sender, receiver) = unbounded::<String>();
    let handle = std::thread::Builder::new()
        .name("drop_trace_writer".into())
        .spawn(move || {
            let mut w = BufWriter::new(file);
            let mut since_flush = 0u32;
            // Flush when caught up (sparse drops → tail always on disk) and batch
            // under a flood (every 256 lines) so the writer keeps pace without
            // an fsync per line.
            while let Ok(line) = receiver.recv() {
                if w.write_all(line.as_bytes()).is_err() || w.write_all(b"\n").is_err() {
                    break;
                }
                since_flush += 1;
                if since_flush >= 256 || receiver.is_empty() {
                    let _ = w.flush();
                    since_flush = 0;
                }
            }
            let _ = w.flush();
        })?;
    Ok((sender, handle))
}

/// True when drop tracing is active. Use to gate trace-only work (extra loops)
/// that would otherwise cost time in the hot path when tracing is off.
#[inline]
pub fn enabled() -> bool {
    state().is_some()
}

fn matches(f: &Filter, record_sig: &str, sub_sig: &str) -> bool {
    match f {
        Filter::All => true,
        Filter::Rules(rules) => rules.iter().any(|(rec, sub)| {
            rec.eq_ignore_ascii_case(record_sig)
                && sub
                    .as_deref()
                    .is_none_or(|s| s.eq_ignore_ascii_case(sub_sig))
        }),
    }
}

/// Object-ids under active investigation for the FO76→FO4 DLC-inherited-keyword
/// delinkage — OMOD paints/mods appearing on unrelated weapons because the
/// weapon-family keyword reference is lost (all FO4 DLC masters end up
/// `used=false`). Trace sites gate on `is_dlc_kw_watched` so a regen with
/// `MODBOX_TRACE_DROPS=1` surfaces ONLY this reference's lifecycle across the
/// mapper + every null/drop choke point. See memory
/// `reference_fo76fo4_omod_wrong_weapon_dlc_keyword_delinkage`.
pub const DLC_KW_WATCHED_LOCALS: &[u32] = &[
    0x0011_3855, // FO76 DLC04_ma_HandmadeAssaultRifle — family keyword (source)
    0x0003_3B61, // DLCNukaWorld DLC04_ma_HandmadeAssaultRifle — family keyword (FO4 target)
    0x005C_44E6, // ATX_mod_HandMadeGun paint OMOD (ScreamingEagle_Wood)
    0x006F_5790, // meltdown / V63 Laser Carbine — wrong weapon showing handmade paints
    0x0011_3854, // FO76 DLC04_HandMadeGun weapon — the weapon that SHOULD carry the keyword
];

/// True when `local` (masked to its object-id) is one of the DLC-keyword
/// investigation ids. Cheap membership test; call only after `enabled()`.
#[inline]
pub fn is_dlc_kw_watched(local: u32) -> bool {
    DLC_KW_WATCHED_LOCALS.contains(&(local & 0x00FF_FFFF))
}

/// Record one drop. `sub_sig` is `""` for a whole-record drop. `local` is the
/// record's local object id (may be the source id if the target FK isn't
/// allocated yet at this stage). Non-blocking: formats and hands the line to the
/// writer thread; never touches disk on the calling thread.
#[inline]
pub fn trace(stage: &str, record_sig: &str, local: u32, sub_sig: &str, reason: &str) {
    let Some(s) = state() else {
        return;
    };
    if matches(&s.filter, record_sig, sub_sig) {
        // Channel is unbounded → send never blocks the conversion worker.
        let _ = s.sender.send(format!(
            "[drop_trace] stage={stage} rec={record_sig}:{local:06X} sub={sub_sig} reason={reason}"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_filter_matches_anything() {
        assert!(matches(&Filter::All, "FACT", "VENC"));
        assert!(matches(&Filter::All, "WEAP", ""));
    }

    #[test]
    fn record_only_rule_matches_any_sub() {
        let f = parse_filter("FACT").unwrap();
        assert!(matches(&f, "FACT", "VENC"));
        assert!(matches(&f, "FACT", "XNAM"));
        assert!(!matches(&f, "QUST", "VENC"));
    }

    #[test]
    fn record_and_sub_rule_is_exact() {
        let f = parse_filter("FACT:VENC").unwrap();
        assert!(matches(&f, "FACT", "VENC"));
        assert!(matches(&f, "fact", "venc")); // case-insensitive
        assert!(!matches(&f, "FACT", "XNAM"));
    }

    #[test]
    fn multi_rule_and_blank_filter() {
        let f = parse_filter("FACT:VENC,QUST").unwrap();
        assert!(matches(&f, "FACT", "VENC"));
        assert!(matches(&f, "QUST", "CNAM"));
        assert!(!matches(&f, "FACT", "XNAM"));
        assert!(parse_filter("  ").is_none());
        assert!(matches!(parse_filter("1").unwrap(), Filter::All));
    }

    #[test]
    fn writer_persists_lines_then_closes_on_sender_drop() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dt.log");
        let (tx, handle) = start_writer(File::create(&path).unwrap()).unwrap();
        tx.send("[drop_trace] one".into()).unwrap();
        tx.send("[drop_trace] two".into()).unwrap();
        drop(tx); // closing the channel ends the writer loop + final flush
        handle.join().unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[drop_trace] one\n[drop_trace] two\n"
        );
    }
}
