//! `TargetHook` trait — per-target-game post-translation hooks.

use super::super::record::Record;
use super::super::sym::StringInterner;
use super::pair_hook::HookResult;

/// Minimal context passed into target hooks.
pub struct TargetCtx<'a> {
    pub interner: &'a StringInterner,
}

/// Hook run on every translated record after all pair-level transforms.
pub trait TargetHook: Send + Sync {
    fn run(&self, ctx: &mut TargetCtx<'_>, record: &mut Record) -> HookResult;
}

/// Default no-op target hook.
pub struct NoOpTargetHook;

impl TargetHook for NoOpTargetHook {
    fn run(&self, _ctx: &mut TargetCtx<'_>, _record: &mut Record) -> HookResult {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode};
    use crate::sym::StringInterner;

    #[test]
    fn noop_target_hook_runs_without_error() {
        let hook = NoOpTargetHook;
        let mut interner = StringInterner::new();
        let mut ctx = TargetCtx {
            interner: &mut interner,
        };
        let fk = FormKey::parse("000800@Mod.esm", ctx.interner).unwrap();
        let mut record = Record::new(SigCode::from_str("WEAP").unwrap(), fk);
        hook.run(&mut ctx, &mut record).unwrap();
    }
}
