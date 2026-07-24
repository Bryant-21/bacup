use super::*;

pub(super) const FO4_MGEF_DATA_LEN: usize = 152;
pub(super) const FO76_MGEF_DATA_LEN: usize = 160;
pub(super) const FO76_MGEF_DATA_WITHOUT_FLAGS2_LEN: usize = 156;
pub(super) const FO76_MGEF_DATA_FLAGS2_OFFSET: usize = 4;
pub(super) const FO76_MGEF_DATA_FLAGS2_END: usize = 8;
pub(super) const FO4_MGEF_DATA_ARCHETYPE_OFFSET: usize = 64;
pub(super) const FO4_MGEF_ARCHETYPE_SCRIPT: u32 = 1;
pub(super) const FO4_MGEF_ARCHETYPE_STAGGER: u32 = 33;
pub(super) const FO76_MGEF_ARCHETYPE_TURBO_FERT: u32 = 50;
pub(super) const FO76_MGEF_ARCHETYPE_CORPSE_HIGHLIGHT: u32 = 51;
pub(super) const FO76_MGEF_ARCHETYPE_STUN: u32 = 52;

/// Record type sigs whose "Effects" subrecord group is treated as a synthetic
/// source field (i.e. the orchestrator synthesizes it rather than decoding it
/// directly from the source ESP).
///
/// RACE also synthesizes `BehaviorGraphDatas`, but that is a YAML-level
/// concept handled by the field-expansion transform, not a subrecord-drop.
pub const EFFECTS_SYNTHETIC_RECORD_SIGS: &[[u8; 4]] = &[*b"ALCH", *b"ENCH", *b"PERK", *b"SPEL"];

/// Hook result pair for effects key routing (field name sym → target key sym).
///
/// When `None`, no rerouting is needed. When `Some((field_sig, target_sig))`,
/// the orchestrator should use `target_sig` as the target subrecord sig for
/// the field identified by `field_sig`.
///
/// Mirrors `Fo76ToFo4Hooks::translate_effects_keys`.
pub struct EffectsKeyRoute {
    /// The field sig to match on the source record.
    pub field_sig: SubrecordSig,
    /// The target subrecord sig to emit.
    pub target_sig: SubrecordSig,
}
impl Fo76Fo4Hook {
    pub(super) fn convert_mgef_data_to_fo4_layout(record: &mut Record) {
        if record.sig.0 != *b"MGEF" {
            return;
        }
        for entry in &mut record.fields {
            if entry.sig.0 != *b"DATA" {
                continue;
            }
            let FieldValue::Bytes(bytes) = &mut entry.value else {
                continue;
            };
            match bytes.len() {
                FO76_MGEF_DATA_LEN => {
                    bytes.drain(FO76_MGEF_DATA_FLAGS2_OFFSET..FO76_MGEF_DATA_FLAGS2_END);
                    bytes.truncate(FO4_MGEF_DATA_LEN);
                }
                FO76_MGEF_DATA_WITHOUT_FLAGS2_LEN => {
                    bytes.truncate(FO4_MGEF_DATA_LEN);
                }
                _ => {}
            }
            Self::normalize_mgef_archetype(bytes.as_mut_slice());
        }
    }

    pub(super) fn normalize_mgef_archetype(bytes: &mut [u8]) {
        let Some(chunk) =
            bytes.get_mut(FO4_MGEF_DATA_ARCHETYPE_OFFSET..FO4_MGEF_DATA_ARCHETYPE_OFFSET + 4)
        else {
            return;
        };
        let archetype = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let normalized = match archetype {
            FO76_MGEF_ARCHETYPE_STUN => FO4_MGEF_ARCHETYPE_STAGGER,
            FO76_MGEF_ARCHETYPE_TURBO_FERT | FO76_MGEF_ARCHETYPE_CORPSE_HIGHLIGHT => {
                FO4_MGEF_ARCHETYPE_SCRIPT
            }
            value if value > FO4_MAX_MGEF_ARCHETYPE => FO4_MGEF_ARCHETYPE_SCRIPT,
            _ => return,
        };
        chunk.copy_from_slice(&normalized.to_le_bytes());
    }

    pub(super) fn drop_perk_vmad(record: &mut Record) {
        if record.sig.0 != *b"PERK" {
            return;
        }
        record.fields.retain(|entry| entry.sig.0 != *b"VMAD");
    }

    /// Returns `true` if this record type synthesizes its `Effects` group.
    ///
    /// Called by the orchestrator before field dispatch to decide whether to
    /// decode the Effects subrecords from the source ESP or synthesize them.
    pub fn is_effects_synthetic(record_sig: SigCode) -> bool {
        EFFECTS_SYNTHETIC_RECORD_SIGS
            .iter()
            .any(|sig| record_sig.0 == *sig)
    }

    /// Returns the effects key rerouting for the given record type and field,
    /// or `None` if no rerouting is needed.
    ///
    /// Mirrors `Fo76ToFo4Hooks::translate_effects_keys`.
    ///
    /// For ALCH/ENCH/SPEL: DATA/EFID/EffectData → Effects::EFID
    /// For PERK: DATA → Effects::DATA
    pub fn translate_effects_key(
        record_sig: SigCode,
        field_sig: SubrecordSig,
    ) -> Option<EffectsKeyRoute> {
        // Only applies when the record has an Effects group.
        if !Self::is_effects_synthetic(record_sig) {
            return None;
        }
        match &record_sig.0 {
            b"ALCH" | b"ENCH" | b"SPEL" => {
                // DATA, EFID, EFIT (EffectData) → Effects / EFID
                match &field_sig.0 {
                    b"DATA" | b"EFID" | b"EFIT" => Some(EffectsKeyRoute {
                        field_sig,
                        target_sig: SubrecordSig(*b"EFID"),
                    }),
                    _ => None,
                }
            }
            b"PERK" => {
                // DATA → Effects / DATA
                match &field_sig.0 {
                    b"DATA" => Some(EffectsKeyRoute {
                        field_sig,
                        target_sig: SubrecordSig(*b"DATA"),
                    }),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}
