//! `PairHook` trait — per-(source, target) game-pair hooks.

use super::super::ids::FormKey;
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
    pub source_plugin_name: Option<&'a str>,
    pub source_master_names: &'a [String],
    pub target_master_names: &'a [String],
}

impl<'a> PairCtx<'a> {
    pub fn new(interner: &'a StringInterner) -> Self {
        Self {
            interner,
            source_plugin_name: None,
            source_master_names: &[],
            target_master_names: &[],
        }
    }

    pub fn with_source(
        interner: &'a StringInterner,
        source_plugin_name: &'a str,
        source_master_names: &'a [String],
    ) -> Self {
        Self {
            interner,
            source_plugin_name: Some(source_plugin_name),
            source_master_names,
            target_master_names: &[],
        }
    }

    pub fn with_source_and_target(
        interner: &'a StringInterner,
        source_plugin_name: &'a str,
        source_master_names: &'a [String],
        target_master_names: &'a [String],
    ) -> Self {
        Self {
            interner,
            source_plugin_name: Some(source_plugin_name),
            source_master_names,
            target_master_names,
        }
    }

    pub fn resolve_source_form_id(&self, raw_form_id: u32) -> Option<FormKey> {
        if raw_form_id == 0 {
            return None;
        }
        let source_plugin_name = self.source_plugin_name?;
        let source_index = ((raw_form_id >> 24) & 0xFF) as usize;
        let plugin_name = if source_index == self.source_master_names.len() {
            source_plugin_name
        } else {
            self.source_master_names.get(source_index)?.as_str()
        };
        Some(FormKey {
            local: raw_form_id & 0x00FF_FFFF,
            plugin: self.interner.intern(plugin_name),
        })
    }
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
        let mut ctx = PairCtx::new(&mut interner);
        let fk = FormKey::parse("000800@Mod.esm", ctx.interner).unwrap();
        let mut record = Record::new(SigCode::from_str("WEAP").unwrap(), fk);
        hook.pre_translate(&mut ctx, &mut record).unwrap();
        hook.post_translate(&mut ctx, &mut record).unwrap();
        let synth = hook.synthesize_records(&mut ctx);
        assert!(synth.is_empty());
    }

    #[test]
    fn source_form_id_resolution_is_context_local_and_strict() {
        let interner = StringInterner::new();
        let fnv_masters = vec!["FalloutNV.esm".to_string()];
        let fo3_masters = vec!["Fallout3.esm".to_string(), "Anchorage.esm".to_string()];
        let fnv = PairCtx::with_source(&interner, "DeadMoney.esm", &fnv_masters);
        let fo3 = PairCtx::with_source(&interner, "ThePitt.esm", &fo3_masters);

        let fnv_master = fnv.resolve_source_form_id(0x0000_1234).unwrap();
        let fnv_own = fnv.resolve_source_form_id(0x0100_1234).unwrap();
        let fo3_second_master = fo3.resolve_source_form_id(0x0100_1234).unwrap();
        let fo3_own = fo3.resolve_source_form_id(0x0200_1234).unwrap();

        assert_eq!(interner.resolve(fnv_master.plugin), Some("FalloutNV.esm"));
        assert_eq!(interner.resolve(fnv_own.plugin), Some("DeadMoney.esm"));
        assert_eq!(
            interner.resolve(fo3_second_master.plugin),
            Some("Anchorage.esm")
        );
        assert_eq!(interner.resolve(fo3_own.plugin), Some("ThePitt.esm"));
        assert!(fnv.resolve_source_form_id(0x0200_1234).is_none());
        assert!(fo3.resolve_source_form_id(0).is_none());
    }
}
