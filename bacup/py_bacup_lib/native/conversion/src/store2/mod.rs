//! Record store v2.
//!
//! `source::SourceEsm` is the mmap-backed raw source store (file-backed pages,
//! compact in-RAM index, transient decompress-on-touch). The target store is
//! the plugin-handle `ParsedRecord` store. `translate_v2` splits the per-record
//! pipeline into parallel prepare/finish passes around serial FormKey-assignment
//! and encode passes (see its module docs).

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
