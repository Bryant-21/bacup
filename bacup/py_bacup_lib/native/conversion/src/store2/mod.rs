//! Record store v2.
//!
//! `source::SourceEsm` is the mmap-backed raw source store (file-backed pages;
//! compact in-RAM index; transient decompress-on-touch). The *target* store
//! remains the plugin-handle `ParsedRecord` store; store2 grows its own target
//! arena with the sinks. `translate_v2` runs the per-record pipeline split into
//! parallel prepare/finish passes around serial FormKey-assignment and encode
//! passes (see translate_v2.rs docs for the exact pass contract).

pub mod fixups_v2;
pub mod source;
#[cfg(test)]
pub(crate) mod test_util;
pub mod translate_v2;
pub mod visitor;
pub mod visitors;

pub use source::{RecordIndexEntry2, RecordView, SourceEsm, SourceOpenError};
pub use visitor::{
    GatherOutput, Lane, MasterScanCache, RecordVisitor, SubrecordPatch, Sweep, SweepCtx,
    VisitOutcome, run_sweep,
};
