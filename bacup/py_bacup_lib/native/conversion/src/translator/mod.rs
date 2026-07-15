//! Translator orchestrator — top-level skeleton.
//!
//! `Translator` drives the per-record translation pipeline:
//!   1. PairHook::pre_translate
//!   2. Field rewrites + named transforms (from TranslationMaps + TransformRegistry)
//!   3. PairHook::post_translate
//!   4. TargetHook::run
//!
//! Field rewrites, drop lists, and named transforms are dispatched from
//! `TranslationMaps`; per-pair and per-target hooks are wired via
//! `pair_hook_for` / `target_hook_for`.

pub mod ammo_substitute;
pub mod class_a_normalize;
pub mod maps;
pub mod pair_hook;
pub mod pair_hooks;
pub mod target_hook;
pub mod target_hooks;
pub mod transforms;

use super::errors::ConfigError;
use super::ids::SubrecordSig;
use super::record::Record;
use super::sym::{StringInterner, Sym};
use maps::TranslationMaps;
use pair_hook::{NoOpPairHook, PairHook};
use target_hook::{NoOpTargetHook, TargetHook};
use transforms::{TransformCtx, TransformRegistry};

fn pair_hook_for(source: Game, target: Game) -> Box<dyn PairHook> {
    match (source, target) {
        (Game::Fo3, Game::Fo4) => Box::new(pair_hooks::fnv_fo4::Fo3Fo4Hook),
        (Game::Fnv, Game::Fo4) => Box::new(pair_hooks::fnv_fo4::FnvFo4Hook),
        (Game::Fo76, Game::Fo4) => Box::new(pair_hooks::fo76_fo4::Fo76Fo4Hook),
        (Game::SkyrimSe, Game::Fo4) => Box::new(pair_hooks::skyrimse_fo4::SkyrimSeFo4Hook),
        _ => Box::new(NoOpPairHook),
    }
}

fn target_hook_for(target: Game) -> Box<dyn TargetHook> {
    match target {
        Game::Fo4 => Box::new(target_hooks::fo4::Fo4TargetHook),
        _ => Box::new(NoOpTargetHook),
    }
}

/// Supported Bethesda games for the conversion pipeline.
///
/// Serialised game strings (used as map-file name components) are produced by
/// `Game::as_str()`. `Game::from_str` matches the same lowercase identifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Game {
    Fo3,
    Fnv,
    Fo4,
    Fo76,
    Skyrim,
    SkyrimSe,
    Starfield,
    Oblivion,
}

impl Game {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "fo3" => Some(Self::Fo3),
            "fnv" => Some(Self::Fnv),
            "fo4" => Some(Self::Fo4),
            "fo76" => Some(Self::Fo76),
            "skyrim" => Some(Self::Skyrim),
            "skyrimse" => Some(Self::SkyrimSe),
            "starfield" => Some(Self::Starfield),
            "oblivion" => Some(Self::Oblivion),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fo3 => "fo3",
            Self::Fnv => "fnv",
            Self::Fo4 => "fo4",
            Self::Fo76 => "fo76",
            Self::Skyrim => "skyrim",
            Self::SkyrimSe => "skyrimse",
            Self::Starfield => "starfield",
            Self::Oblivion => "oblivion",
        }
    }
}

/// A deferred-translation reason — records that need a separate pipeline pass.
#[derive(Debug)]
pub enum DeferredKind {
    /// FNV legacy Papyrus scripting requires a separate script-port pass.
    FnvLegacyScripting,
    /// Record requires the V2 pipeline (not yet implemented).
    V2Pipeline,
}

/// Minimal decision attached to a Dropped or Deferred result.
#[derive(Debug)]
pub struct Decision {
    pub kind: Sym,
    pub message: String,
}

/// The outcome of translating one record.
#[derive(Debug)]
pub enum TranslateResult {
    /// Record was translated; the inner `Record` is the translated version.
    Translated(Record),
    /// Record was explicitly dropped (e.g. in the skip list or by a hook).
    Dropped { reason: Sym, decision: Decision },
    /// Record cannot be translated in this pass — needs a different pipeline.
    Deferred(DeferredKind),
}

/// Orchestrates record translation for one (source, target) game pair.
///
/// Holds:
/// - The loaded `TranslationMaps` for the pair.
/// - A `TransformRegistry` with registered named transforms.
/// - A `PairHook` (game-pair-specific logic, defaults to no-op).
/// - A `TargetHook` (target-game post-processing, defaults to no-op).
pub struct Translator {
    pub source: Game,
    pub target: Game,
    pub maps: TranslationMaps,
    pub transforms: TransformRegistry,
    pair_hook: Box<dyn PairHook>,
    target_hook: Box<dyn TargetHook>,
}

impl Translator {
    /// Create a new `Translator` for the given game pair.
    ///
    /// Loads the YAML translation map (if any) and builds an empty
    /// `TransformRegistry`. Installs the pair hook and target hook registered
    /// for (source, target) via `pair_hook_for` / `target_hook_for`, defaulting
    /// to no-op when none are registered.
    pub fn new(source: Game, target: Game) -> Result<Self, ConfigError> {
        Ok(Self {
            source,
            target,
            maps: TranslationMaps::load(source, target)?,
            transforms: transforms::default_registry(),
            pair_hook: pair_hook_for(source, target),
            target_hook: target_hook_for(target),
        })
    }

    /// Run the pre-translate pair hook.
    pub fn pre_translate(
        &self,
        ctx: &mut pair_hook::PairCtx<'_>,
        record: &mut Record,
    ) -> pair_hook::HookResult {
        self.pair_hook.pre_translate(ctx, record)
    }

    /// Run the post-translate pair hook.
    pub fn post_translate(
        &self,
        ctx: &mut pair_hook::PairCtx<'_>,
        record: &mut Record,
    ) -> pair_hook::HookResult {
        self.pair_hook.post_translate(ctx, record)
    }

    /// Run the target hook.
    pub fn run_target_hook(
        &self,
        ctx: &mut target_hook::TargetCtx<'_>,
        record: &mut Record,
    ) -> pair_hook::HookResult {
        self.target_hook.run(ctx, record)
    }

    /// Translate one record.
    ///
    /// Applies map-driven field rewrites and named transforms. Returns:
    /// - `Translated` on success.
    /// - `Dropped` when the record sig is in `skip_records`.
    /// - `Deferred(FnvLegacyScripting)` when the map delegates to that pass.
    pub fn translate(&self, record: &Record, interner: &StringInterner) -> TranslateResult {
        self.translate_with_skip_override(record, interner, None)
    }

    /// Translate one record while bypassing the map skip list for one signature.
    ///
    /// Structured emitters use this for records that are skipped by the generic
    /// top-level translator but handled by a dedicated writer.
    pub fn translate_ignoring_skip(
        &self,
        record: &Record,
        interner: &StringInterner,
        ignored_signature: &str,
    ) -> TranslateResult {
        self.translate_with_skip_override(record, interner, Some(ignored_signature))
    }

    fn translate_with_skip_override(
        &self,
        record: &Record,
        interner: &StringInterner,
        ignored_signature: Option<&str>,
    ) -> TranslateResult {
        let sig = record.sig.as_str();

        // Skip list.
        if ignored_signature != Some(sig) && self.maps.skip_records.contains(sig) {
            let kind = interner.intern("skip_records");
            let reason = kind;
            return TranslateResult::Dropped {
                reason,
                decision: Decision {
                    kind,
                    message: format!("sig {sig} in skip_records"),
                },
            };
        }

        let mut out = record.clone();

        // Look up the per-sig record map.
        let map = match self.maps.record_map(sig) {
            Some(m) => m,
            None => return TranslateResult::Translated(out),
        };

        // Apply target sig override.
        if let Some(ref tgt_sig) = map.target_sig {
            if let Ok(new_sig) = super::ids::SigCode::from_str(tgt_sig) {
                out.sig = new_sig;
            }
        }

        // Drop fields listed in drop_fields.
        if !map.drop_fields.is_empty() {
            let rec_sig = out.sig;
            let rec_local = out.form_key.local;
            out.fields.retain(|f| {
                let drop = map.drop_fields.iter().any(|d| d.as_str() == f.sig.as_str());
                if drop {
                    crate::drop_trace::trace(
                        "translate.drop_field",
                        rec_sig.as_str(),
                        rec_local,
                        f.sig.as_str(),
                        "sig in translation-map drop list",
                    );
                }
                !drop
            });
        }

        // Field rewrites: rename source_field → target_field.
        for rewrite in &map.field_rewrites {
            let src_sig = match SubrecordSig::from_str(&rewrite.source_field) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let tgt_sig = match SubrecordSig::from_str(&rewrite.target_field) {
                Ok(s) => s,
                Err(_) => continue,
            };
            for entry in out.fields.iter_mut() {
                if entry.sig == src_sig {
                    entry.sig = tgt_sig;
                    break;
                }
            }
        }

        // Transform invocations.
        let mut ctx = TransformCtx { interner };
        for invocation in &map.transforms {
            let Some(transform) = self.transforms.get(&invocation.name) else {
                continue;
            };
            let field_sig = match SubrecordSig::from_str(&invocation.field) {
                Ok(s) => s,
                Err(_) => continue,
            };
            for entry in out.fields.iter_mut() {
                if entry.sig == field_sig {
                    let _ = transform.apply(&mut ctx, &mut entry.value, &invocation.config);
                }
            }
        }

        TranslateResult::Translated(out)
    }
}

#[cfg(test)]
mod scol_tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record};
    use smallvec::SmallVec;

    fn source_fk(interner: &StringInterner, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern("SeventySix.esm"),
        }
    }

    #[test]
    fn fo76_scol_map_converts_every_repeated_onam_and_drops_invalid_fields() {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("SCOL").unwrap(),
            source_fk(&mut interner, 0x294744),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XALG").unwrap(),
            value: FieldValue::Uint(1),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("ONAM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_slice(&[0x12, 0x58, 0x03, 0x00, 0, 0, 0, 0])),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("ONAM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_slice(&[0x30, 0x47, 0x03, 0x00, 0, 0, 0, 0])),
        });

        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated SCOL, got {other:?}"),
        };

        assert!(
            translated
                .fields
                .iter()
                .all(|field| field.sig.as_str() != "XALG")
        );

        let onam_locals: Vec<_> = translated
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "ONAM")
            .filter_map(|field| match &field.value {
                FieldValue::FormKey(fk) => Some(fk.local),
                _ => None,
            })
            .collect();
        assert_eq!(onam_locals, vec![0x035812, 0x034730]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue};
    use crate::sym::StringInterner;
    use smallvec::SmallVec;

    #[test]
    fn translator_skeleton_translates_passthrough() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("000800@Mod.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("WEAP").unwrap(), fk);
        record.eid = Some(interner.intern("PassThrough"));

        let result = translator.translate(&record, &mut interner);
        match result {
            TranslateResult::Translated(t) => {
                assert_eq!(t.sig, record.sig);
                assert_eq!(t.eid, record.eid);
            }
            _ => panic!("expected Translated"),
        }
    }

    #[test]
    fn fnv_fo4_race_drops_legacy_head_body_and_facegen_subrecords() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fnv, Game::Fo4).unwrap();
        let fk = FormKey::parse("0987DF@FalloutNV.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("RACE").unwrap(), fk);

        for sig in [
            "EDID", "FULL", "DESC", "DATA", "NAM0", "MNAM", "INDX", "MODL", "MODT", "ICON", "FNAM",
            "NAM1", "MNAM", "INDX", "MODL", "MODT", "FNAM", "HNAM", "ENAM", "MNAM", "FGGS", "FGGA",
            "FGTS", "SNAM", "FNAM", "HEAD", "MICO",
        ] {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str(sig).unwrap(),
                value: FieldValue::None,
            });
        }

        let translated = match translator.translate(&record, &interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated RACE, got {other:?}"),
        };

        assert_eq!(
            translated
                .fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["EDID", "FULL", "DESC"]
        );
    }

    #[test]
    fn fo76_keym_retains_fo4_compatible_name_preview_and_sounds() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("27DB2F@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("KEYM").unwrap(), fk);

        for (sig, bytes) in [
            ("FULL", b"Congressional Access Card\0".as_slice()),
            ("PTRN", &0x0024_8895_u32.to_le_bytes()),
            ("YNAM", &0x0059_5D2B_u32.to_le_bytes()),
            ("ZNAM", &0x0059_5D2C_u32.to_le_bytes()),
            ("XALG", &1_u32.to_le_bytes()),
        ] {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str(sig).unwrap(),
                value: FieldValue::Bytes(SmallVec::from_slice(bytes)),
            });
        }

        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated KEYM, got {other:?}"),
        };

        assert_eq!(
            translated
                .fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["FULL", "PTRN", "YNAM", "ZNAM"]
        );
    }

    #[test]
    fn fo76_fo4_race_preserves_sraf_subgraph_role() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("00D191@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("RACE").unwrap(), fk);

        for (sig, bytes) in [
            (
                "SGNM",
                b"Actors\\Snallygaster\\Behaviors\\SnallygasterCoreBehavior.hkx\0".as_slice(),
            ),
            ("SAPT", b"Actors\\Snallygaster\\Animations\0".as_slice()),
            ("SRAF", &[1, 0, 0, 0]),
            ("SAKD", &[0, 0, 0, 0]),
            ("STKD", &[0, 0, 0, 0]),
        ] {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str(sig).unwrap(),
                value: FieldValue::Bytes(SmallVec::from_slice(bytes)),
            });
        }

        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated RACE, got {other:?}"),
        };
        let sigs: Vec<_> = translated
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();

        assert_eq!(sigs, vec!["SGNM", "SAPT", "SRAF", "SAKD", "STKD"]);
    }

    #[test]
    fn fo76_currency_records_translate_to_fo4_misc() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("3F7410@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("CNCY").unwrap(), fk);
        record.eid = Some(interner.intern("LegendaryTokens"));

        let result = translator.translate(&record, &mut interner);
        match result {
            TranslateResult::Translated(t) => {
                assert_eq!(t.sig.as_str(), "MISC");
                assert_eq!(t.eid, record.eid);
            }
            other => panic!("expected translated CNCY, got {other:?}"),
        }
    }

    #[test]
    fn fo76_addn_ikek_translates_to_fo4_data_index() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("84F665@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("ADDN").unwrap(), fk);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("IKEK").unwrap(),
            value: FieldValue::Uint(185_746_299),
        });

        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated ADDN, got {other:?}"),
        };

        assert!(
            translated
                .fields
                .iter()
                .all(|field| field.sig.as_str() != "IKEK")
        );
        let data = translated
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DATA")
            .expect("IKEK should become FO4 DATA");
        assert_eq!(data.value, FieldValue::Uint(185_746_299));
    }

    #[test]
    fn fo76_lvli_retains_lvlf_use_all_flag() {
        // Outfit-combining leveled lists (headwear + clothes) rely on the
        // "Use All" flag (LVLF bit 0x04) to return every component. Dropping
        // LVLF made FO4 pick only one branch → naked NPCs.
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("6CC09E@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("LVLI").unwrap(), fk);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("LVLF").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_slice(&[4])),
        });

        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated LVLI, got {other:?}"),
        };

        let lvlf = translated
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "LVLF")
            .expect("LVLI LVLF flag must be carried to FO4");
        assert_eq!(lvlf.value, FieldValue::Bytes(SmallVec::from_slice(&[4])));
    }

    #[test]
    fn fo76_ligh_lils_drops_without_mapping_to_fo4_fnam() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("15AE5B@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("LIGH").unwrap(), fk);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("LILS").unwrap(),
            value: FieldValue::Float(0.0),
        });

        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated LIGH, got {other:?}"),
        };

        assert!(
            translated
                .fields
                .iter()
                .all(|field| field.sig.as_str() != "LILS"),
            "FO76 LILS must not survive into FO4 output"
        );
        assert!(
            translated
                .fields
                .iter()
                .all(|field| field.sig.as_str() != "FNAM"),
            "FO76 LILS is not equivalent to FO4 FNAM"
        );
    }

    #[test]
    fn fo76_cont_translation_preserves_items_and_strips_zero_health_destructibles() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("11CEED@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("CONT").unwrap(), fk);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("COCT").unwrap(),
            value: FieldValue::Uint(2),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_slice(&[1, 0, 0, 0, 0])),
        });
        for item in [0x0673B5_u32, 0x59DD1D] {
            let mut cnto = Vec::new();
            cnto.extend_from_slice(&item.to_le_bytes());
            cnto.extend_from_slice(&1_u32.to_le_bytes());
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("CNTO").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_vec(cnto)),
            });
        }

        let mut zero_health_dest = Vec::new();
        zero_health_dest.extend_from_slice(&0_i32.to_le_bytes());
        zero_health_dest.extend_from_slice(&[1, 0, 0, 0]);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DEST").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(zero_health_dest)),
        });
        for sig in ["DSTD", "DSTF"] {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str(sig).unwrap(),
                value: FieldValue::Bytes(SmallVec::from_slice(&[0; 8])),
            });
        }

        let mut nonzero_health_dest = Vec::new();
        nonzero_health_dest.extend_from_slice(&25_i32.to_le_bytes());
        nonzero_health_dest.extend_from_slice(&[1, 0, 0, 0]);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DEST").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(nonzero_health_dest)),
        });
        for sig in ["DSTD", "DSTF"] {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str(sig).unwrap(),
                value: FieldValue::Bytes(SmallVec::from_slice(&[1; 8])),
            });
        }

        let mut ctx = pair_hook::PairCtx {
            interner: &interner,
        };
        translator.pre_translate(&mut ctx, &mut record).unwrap();
        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated CONT, got {other:?}"),
        };

        assert_eq!(
            translated
                .fields
                .iter()
                .filter(|field| field.sig.as_str() == "CNTO")
                .count(),
            2
        );
        assert!(translated.fields.iter().any(|field| {
            field.sig.as_str() == "DATA"
                && field.value == FieldValue::Bytes(SmallVec::from_slice(&[1, 0, 0, 0, 0]))
        }));
        assert_eq!(
            translated
                .fields
                .iter()
                .filter(|field| field.sig.as_str() == "DEST")
                .count(),
            1,
            "only zero-health FO76 container destructible groups should be stripped"
        );
        assert_eq!(
            translated
                .fields
                .iter()
                .filter(|field| matches!(field.sig.as_str(), "DSTD" | "DSTF"))
                .count(),
            2,
            "the nonzero-health destructible stage should remain"
        );
        let first_cnto = translated
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "CNTO")
            .expect("CONT keeps CNTO");
        let FieldValue::Struct(fields) = &first_cnto.value else {
            panic!("CNTO should be structured");
        };
        assert!(matches!(
            fields
                .iter()
                .find(|(key, _)| interner.resolve(*key) == Some("item"))
                .map(|(_, value)| value),
            Some(FieldValue::FormKey(form_key)) if form_key.local == 0x0673B5
        ));
    }

    #[test]
    fn fo76_furn_translation_preserves_inventory_items() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("7AAD19@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("FURN").unwrap(), fk);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("COCT").unwrap(),
            value: FieldValue::Uint(2),
        });
        for item in [0x387B02_u32, 0x7AEDA1] {
            let mut cnto = Vec::new();
            cnto.extend_from_slice(&item.to_le_bytes());
            cnto.extend_from_slice(&1_u32.to_le_bytes());
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("CNTO").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_vec(cnto)),
            });
        }

        let mut ctx = pair_hook::PairCtx {
            interner: &interner,
        };
        translator.pre_translate(&mut ctx, &mut record).unwrap();
        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated FURN, got {other:?}"),
        };

        assert_eq!(
            translated
                .fields
                .iter()
                .filter(|field| field.sig.as_str() == "CNTO")
                .count(),
            2
        );
        let first_cnto = translated
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "CNTO")
            .expect("FURN keeps CNTO");
        let FieldValue::Struct(fields) = &first_cnto.value else {
            panic!("CNTO should be structured");
        };
        assert!(matches!(
            fields
                .iter()
                .find(|(key, _)| interner.resolve(*key) == Some("item"))
                .map(|(_, value)| value),
            Some(FieldValue::FormKey(form_key)) if form_key.local == 0x387B02
        ));
    }

    #[test]
    fn fo76_npc_translation_keeps_head_part_pnam_rows() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("5858E7@SeventySix.esm", &mut interner).unwrap();
        let source_beard = FormKey::parse("135AA4@SeventySix.esm", &mut interner).unwrap();
        let target_beard = FormKey::parse("135AA4@Fallout4.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("NPC_").unwrap(), fk);
        record.eid = Some(interner.intern("W05_LvlDenizen_RaiderLooterM_or_LiteAlly"));
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(interner.intern("W05_LvlDenizen_RaiderLooterM_or_LiteAlly")),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("PNAM").unwrap(),
            value: FieldValue::FormKey(source_beard),
        });

        let mut translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated NPC_, got {other:?}"),
        };

        let pnam_count = translated
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "PNAM")
            .count();
        assert_eq!(
            pnam_count, 1,
            "FO76 NPC_ source head part should be preserved for mapper/fallback handling"
        );
        let hdpt_sig = SigCode::from_str("HDPT").unwrap();
        let beard_eid = interner.intern("Beard12");
        let mut mapper = FormKeyMapper::new(
            [(beard_eid, target_beard, hdpt_sig)],
            MapperOptions {
                output_plugin_name: "SeventySix.esm".to_string(),
                target_master_names: vec!["Fallout4.esm".to_string()],
                use_base_game_assets: true,
                preserve_source_ids: true,
                ..MapperOptions::default()
            },
            &interner,
        );
        mapper.allocate_or_resolve(source_beard, Some(beard_eid), hdpt_sig);
        mapper.rewrite_record(&mut translated).unwrap();

        assert!(matches!(
            translated
                .fields
                .iter()
                .find(|field| field.sig.as_str() == "PNAM")
                .map(|field| &field.value),
            Some(FieldValue::FormKey(form_key)) if *form_key == target_beard
        ));
    }

    #[test]
    fn fo76_wrld_pipeline_drops_source_runtime_tables() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("00DC6C@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("WRLD").unwrap(), fk);
        record.eid = Some(interner.intern("TESTNewTerrain"));
        for sig in ["EDID", "RNAM", "MHDT", "OFST", "CLSZ", "NAM0"] {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str(sig).unwrap(),
                value: FieldValue::Bytes(SmallVec::from_slice(&[0, 1, 2, 3])),
            });
        }

        let mut ctx = pair_hook::PairCtx {
            interner: &interner,
        };
        translator.pre_translate(&mut ctx, &mut record).unwrap();
        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated WRLD, got {other:?}"),
        };
        let sigs: Vec<_> = translated
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();

        assert_eq!(sigs, vec!["EDID", "NAM0"]);
    }

    #[test]
    fn fo76_omod_data_transform_filters_raw_int_properties() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("7745A6@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("OMOD").unwrap(), fk);
        record.eid = Some(interner.intern("ATX_mod_BackPack_TheHoldAll_Material_Default"));

        let form_type = interner.intern("FormType");
        let properties = interner.intern("Properties");
        let property = interner.intern("Property");
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Struct(vec![
                (form_type, FieldValue::Uint(1_330_467_393)),
                (
                    properties,
                    FieldValue::List(vec![
                        FieldValue::Struct(vec![(property, FieldValue::Uint(15))]),
                        FieldValue::Struct(vec![(property, FieldValue::Uint(13))]),
                        FieldValue::Struct(vec![(property, FieldValue::Uint(3))]),
                    ]),
                ),
            ]),
        });

        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated OMOD, got {other:?}"),
        };
        let data = translated
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DATA")
            .expect("translated OMOD keeps Data field");
        let FieldValue::Struct(data_fields) = &data.value else {
            panic!("expected Data struct");
        };
        let (_, properties_value) = data_fields
            .iter()
            .find(|(key, _)| interner.resolve(*key) == Some("Properties"))
            .expect("Data has Properties");
        let FieldValue::List(items) = properties_value else {
            panic!("expected Properties list");
        };
        assert!(items.is_empty());
    }

    #[test]
    fn fo76_scene_records_translated_by_generic_writer_by_default() {
        // SCEN is a flat top-level FO4 record emitted by the generic writer
        // (resolves NOTE\SNAM-Scene). MODBOX_DISABLE_SCEN re-skips it (gate
        // lives in maps.rs::load); not exercised here to avoid process-global
        // env races.
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        assert!(
            !translator.maps.skip_records.contains("SCEN"),
            "SCEN must not be in skip_records by default"
        );
        let fk = FormKey::parse("534F51@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("SCEN").unwrap(), fk);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(interner.intern("TalesFromWestVirginiaHolotape05Scene")),
        });

        match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(out) => {
                assert_eq!(out.sig.as_str(), "SCEN");
            }
            other => panic!("expected translated SCEN, got {other:?}"),
        }
    }

    #[test]
    fn fo76_fact_venp_is_renamed_and_relaid_to_fo4_venv() {
        // FO4 FACT VENV is required; FO76 supplies VENP (different layout). The
        // map must rename VENP→VENV and relayout the bytes (not drop it).
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("4124AA@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("FACT").unwrap(), fk);
        // Canonical FO76 VENP (LC060_WhitespringVendor): end_hour=24, radius=1200,
        // bools 1,1,1, trailing bytes_6=00.
        let mut venp = Vec::new();
        venp.extend_from_slice(&0u16.to_le_bytes());
        venp.extend_from_slice(&24u16.to_le_bytes());
        venp.extend_from_slice(&1200u32.to_le_bytes());
        venp.extend_from_slice(&[1, 1, 1]);
        venp.push(0);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("VENP").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(venp)),
        });

        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated FACT, got {other:?}"),
        };

        assert!(
            translated.fields.iter().all(|f| f.sig.as_str() != "VENP"),
            "VENP must be renamed away"
        );
        let venv = translated
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "VENV")
            .expect("FACT must carry a VENV after translation");
        let FieldValue::Bytes(bytes) = &venv.value else {
            panic!("VENV must be raw FO4-laid-out bytes");
        };
        let expected: [u8; 12] = [0, 0, 24, 0, 0xB0, 0x04, 0, 0, 1, 1, 1, 0];
        assert_eq!(bytes.as_slice(), &expected);
    }

    #[test]
    fn fo76_cobj_dnam_is_renamed_and_relaid_to_fo4_intv() {
        // FO4 COBJ carries the crafting "created object count" in INTV; the FO76
        // source carries it (plus a UI sort priority) in DNAM under a different
        // layout. The map must rename DNAM→INTV and relayout the bytes (not drop
        // it, which lost the count). Example: 05A371 (co_Weapon_Melee_BoxingGlove).
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("05A371@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("COBJ").unwrap(), fk);
        // FO76 DNAM (struct:f,H,B,B): priority_ui_sort_order=2.0, count=3, pad 0,0.
        let mut dnam = Vec::new();
        dnam.extend_from_slice(&2.0f32.to_le_bytes());
        dnam.extend_from_slice(&3u16.to_le_bytes());
        dnam.push(0);
        dnam.push(0);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DNAM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(dnam)),
        });

        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated COBJ, got {other:?}"),
        };

        assert!(
            translated.fields.iter().all(|f| f.sig.as_str() != "DNAM"),
            "DNAM must be renamed away"
        );
        let intv = translated
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "INTV")
            .expect("COBJ must carry an INTV after translation");
        let FieldValue::Bytes(bytes) = &intv.value else {
            panic!("INTV must be raw FO4-laid-out bytes");
        };
        // FO4 INTV (struct:H,H): created_object_count=3, priority=2.
        assert_eq!(bytes.as_slice(), &[3, 0, 2, 0]);
    }

    #[test]
    fn game_from_str_round_trips() {
        for (s, g) in [
            ("fo3", Game::Fo3),
            ("fnv", Game::Fnv),
            ("fo4", Game::Fo4),
            ("fo76", Game::Fo76),
            ("skyrimse", Game::SkyrimSe),
            ("starfield", Game::Starfield),
        ] {
            assert_eq!(Game::from_str(s), Some(g));
            assert_eq!(g.as_str(), s);
        }
    }

    #[test]
    fn game_from_str_unknown_returns_none() {
        assert!(Game::from_str("unknown_game").is_none());
    }

    // -------------------------------------------------------------------------
    // Per-pair smoke tests: Translator::new must succeed for every
    // registered game pair.  YAML-only pairs use NoOpPairHook (the wildcard
    // arm in pair_hook_for); this verifies the map file loads without error.
    // -------------------------------------------------------------------------

    #[test]
    fn translator_loads_fo3_to_fo4() {
        Translator::new(Game::Fo3, Game::Fo4).expect("fo3→fo4 should load");
    }

    #[test]
    fn fo3_to_fo4_uses_narrow_proj_pair_hook() {
        let interner = StringInterner::new();
        let translator = Translator::new(Game::Fo3, Game::Fo4).unwrap();
        let fk = FormKey::parse("000800@Fallout3.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("PROJ").unwrap(), fk);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(vec![0; 68])),
        });

        let mut ctx = pair_hook::PairCtx {
            interner: &interner,
        };
        translator.pre_translate(&mut ctx, &mut record).unwrap();

        let data = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DATA")
            .unwrap();
        let dnam = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DNAM")
            .unwrap();
        assert!(matches!(&data.value, FieldValue::Bytes(bytes) if bytes.is_empty()));
        assert!(matches!(&dnam.value, FieldValue::Bytes(bytes) if bytes.len() == 93));

        let fk = FormKey::parse("001234@Fallout3.esm", &interner).unwrap();
        let mut term = Record::new(SigCode::from_str("TERM").unwrap(), fk);
        term.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("SNAM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(vec![1, 2, 3, 4])),
        });
        term.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("PNAM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(vec![5, 6, 7, 8])),
        });

        translator.pre_translate(&mut ctx, &mut term).unwrap();

        assert_eq!(
            term.fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["SNAM", "PNAM"],
            "FO3 records must not run FNV-specific non-PROJ rewrites"
        );
    }

    #[test]
    fn translator_loads_fo4_to_skyrimse() {
        Translator::new(Game::Fo4, Game::SkyrimSe).expect("fo4→skyrimse should load");
    }

    #[test]
    fn translator_loads_fo76_to_fnv() {
        Translator::new(Game::Fo76, Game::Fnv).expect("fo76→fnv should load");
    }

    #[test]
    fn translator_loads_fo76_to_skyrimse() {
        Translator::new(Game::Fo76, Game::SkyrimSe).expect("fo76→skyrimse should load");
    }

    #[test]
    fn translator_loads_skyrimse_to_fo4() {
        Translator::new(Game::SkyrimSe, Game::Fo4).expect("skyrimse→fo4 should load");
    }

    #[test]
    fn translator_loads_starfield_to_fo4() {
        Translator::new(Game::Starfield, Game::Fo4).expect("starfield→fo4 should load");
    }

    /// Skyrim→SkyrimSE has no YAML map (no Python pair hook either); Translator
    /// must still construct successfully, yielding an empty TranslationMaps.
    #[test]
    fn translator_loads_skyrim_to_skyrimse_no_map() {
        let t = Translator::new(Game::Skyrim, Game::SkyrimSe)
            .expect("skyrim→skyrimse should load even with no YAML map");
        // No skip_records, no record maps — empty maps are valid.
        assert!(t.maps.skip_records.is_empty());
        assert!(t.maps.record_map("WEAP").is_none());
    }

    /// FO3→FNV has no YAML map; same empty-maps check.
    #[test]
    fn translator_loads_fo3_to_fnv_no_map() {
        let t = Translator::new(Game::Fo3, Game::Fnv)
            .expect("fo3→fnv should load even with no YAML map");
        assert!(t.maps.skip_records.is_empty());
        assert!(t.maps.record_map("WEAP").is_none());
    }
}
