//! Env-gated drop tracing for the conversion pipeline.
//!
//! Each instrumented choke point that drops or nulls a subrecord, field,
//! reference, or record logs one line, so a regen shows where data is lost.
//!
//! Lines go to a log file, never the console. Each trace sends its line over a
//! channel to one background writer thread, so workers never contend on a lock.
//! When disabled, every call returns after one atomic load.
//!
//! Env vars (set by `regen.py --drop-trace`):
//!   * `MODBOX_TRACE_DROPS`: filter, required to enable. `1`/`all`, or
//!     comma-separated `SIG` / `SIG:SUB` rules, e.g. `FACT:VENC` or `FACT,QUST`.
//!   * `MODBOX_TRACE_DROPS_FILE`: output path (default `drop_trace.log` in cwd).
//!
//! Line format:
//! ```text
//! [drop_trace] stage=<stage> rec=<SIG>:<local-hex> sub=<SUB> reason=<text>
//! ```

use crossbeam_channel::{Sender, unbounded};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{OnceLock, RwLock};

enum Filter {
    All,
    /// `(record_sig_uppercase, optional sub_sig_uppercase)` rules; a drop matches
    /// if any rule matches.
    Rules(Vec<(String, Option<String>)>),
}

struct DropTrace {
    filter: Filter,
    sender: Option<Sender<String>>,
    writer: Option<std::thread::JoinHandle<()>>,
}

impl Drop for DropTrace {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }
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

static ENABLED: AtomicBool = AtomicBool::new(false);
static INITIALIZED: OnceLock<()> = OnceLock::new();
static STATE: OnceLock<RwLock<Option<DropTrace>>> = OnceLock::new();

fn state() -> &'static RwLock<Option<DropTrace>> {
    STATE.get_or_init(|| RwLock::new(None))
}

fn configure_inner(filter: Option<&str>, output_path: Option<&str>) -> Result<bool, String> {
    ENABLED.store(false, Ordering::Release);
    let next = match filter.and_then(parse_filter) {
        Some(filter) => {
            let path = output_path.unwrap_or("drop_trace.log");
            let file =
                File::create(path).map_err(|error| format!("cannot open {path}: {error}"))?;
            let (sender, writer) =
                start_writer(file).map_err(|error| format!("cannot start writer: {error}"))?;
            Some(DropTrace {
                filter,
                sender: Some(sender),
                writer: Some(writer),
            })
        }
        None => None,
    };
    let enabled = next.is_some();
    *state()
        .write()
        .map_err(|_| "drop trace state lock poisoned".to_string())? = next;
    ENABLED.store(enabled, Ordering::Release);
    Ok(enabled)
}

fn initialize_from_env() {
    INITIALIZED.get_or_init(|| {
        let Some(filter) = std::env::var("MODBOX_TRACE_DROPS").ok() else {
            return;
        };
        let path = std::env::var("MODBOX_TRACE_DROPS_FILE").ok();
        if let Err(error) = configure_inner(Some(&filter), path.as_deref()) {
            eprintln!("[drop_trace] {error}; drop tracing disabled");
        }
    });
}

pub fn configure(filter: Option<&str>, output_path: Option<&str>) -> Result<bool, String> {
    INITIALIZED.get_or_init(|| ());
    configure_inner(filter, output_path)
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
    initialize_from_env();
    ENABLED.load(Ordering::Acquire)
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

/// Object ids traced for the FO76→FO4 DLC-inherited-keyword delinkage: OMOD
/// paints show on unrelated weapons when the weapon-family keyword reference is
/// lost (all FO4 DLC masters end up `used=false`). Trace sites gate on
/// `is_dlc_kw_watched`, so `MODBOX_TRACE_DROPS=1` shows only this reference's
/// path through the mapper and the null/drop choke points.
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
    initialize_from_env();
    if !ENABLED.load(Ordering::Acquire) {
        return;
    }
    let Ok(state) = state().read() else {
        return;
    };
    let Some(s) = state.as_ref() else {
        return;
    };
    if matches(&s.filter, record_sig, sub_sig) {
        // Channel is unbounded → send never blocks the conversion worker.
        if let Some(sender) = s.sender.as_ref() {
            let _ = sender.send(format!(
                "[drop_trace] stage={stage} rec={record_sig}:{local:06X} sub={sub_sig} reason={reason}"
            ));
        }
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

    #[test]
    fn configure_can_enable_disable_and_change_output_files() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first.log");
        let second = dir.path().join("second.log");

        configure(Some("1"), first.to_str()).unwrap();
        trace("one", "FACT", 1, "VENC", "first");
        configure(Some("QUST"), second.to_str()).unwrap();
        trace("two", "FACT", 2, "VENC", "filtered");
        trace("two", "QUST", 3, "CNAM", "second");
        configure(None, None).unwrap();

        assert!(
            std::fs::read_to_string(&first)
                .unwrap()
                .contains("reason=first")
        );
        let second_text = std::fs::read_to_string(&second).unwrap();
        assert!(!second_text.contains("reason=filtered"));
        assert!(second_text.contains("reason=second"));
        assert!(!enabled());
    }
}
