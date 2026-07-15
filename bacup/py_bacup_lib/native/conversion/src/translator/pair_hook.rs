//! `PairHook` trait — per-(source, target) game-pair hooks.

use super::super::record::Record;
use super::super::sym::StringInterner;

/// Error from a hook invocation.
#[derive(Debug)]
pub enum HookError {
    Runtime(String),
}

impl std::fmt::Display for HookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Runtime(s) => write!(f, "hook runtime error: {s}"),
        }
    }
}

impl std::error::Error for HookError {}

/// Result returned by hook methods.
pub type HookResult = Result<(), HookError>;

/// Minimal context passed into pair hooks.
pub struct PairCtx<'a> {
    pub interner: &'a StringInterner,
}

/// Hook called before and after translating each record, and to synthesize
/// new records for the target plugin.
pub trait PairHook: Send + Sync {
    /// Called before field translation. May mutate the record in-place.
    fn pre_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult;
    /// Called after field translation. May mutate the record in-place.
    fn post_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult;
    /// Called once per run to emit synthesized records (e.g. compatibility patches).
    fn synthesize_records(&self, ctx: &mut PairCtx<'_>) -> Vec<Record>;
}

/// Default no-op pair hook.
pub struct NoOpPairHook;

impl PairHook for NoOpPairHook {
    fn pre_translate(&self, _ctx: &mut PairCtx<'_>, _record: &mut Record) -> HookResult {
        Ok(())
    }

    fn post_translate(&self, _ctx: &mut PairCtx<'_>, _record: &mut Record) -> HookResult {
        Ok(())
    }

    fn synthesize_records(&self, _ctx: &mut PairCtx<'_>) -> Vec<Record> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode};
    use crate::sym::StringInterner;

    #[test]
    fn noop_pair_hook_runs_without_error() {
        let hook = NoOpPairHook;
        let mut interner = StringInterner::new();
        let mut ctx = PairCtx {
            interner: &mut interner,
        };
        let fk = FormKey::parse("000800@Mod.esm", ctx.interner).unwrap();
        let mut record = Record::new(SigCode::from_str("WEAP").unwrap(), fk);
        hook.pre_translate(&mut ctx, &mut record).unwrap();
        hook.post_translate(&mut ctx, &mut record).unwrap();
        let synth = hook.synthesize_records(&mut ctx);
        assert!(synth.is_empty());
    }
}
