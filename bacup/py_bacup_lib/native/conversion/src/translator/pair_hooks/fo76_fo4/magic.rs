use super::*;

pub(super) const FO4_MGEF_DATA_LEN: usize = 152;
pub(super) const FO76_MGEF_DATA_LEN: usize = 160;
pub(super) const FO76_MGEF_DATA_WITHOUT_FLAGS2_LEN: usize = 156;
pub(super) const FO76_MGEF_DATA_FLAGS2_OFFSET: usize = 4;
pub(super) const FO76_MGEF_DATA_FLAGS2_END: usize = 8;
pub(super) const FO4_MGEF_DATA_ARCHETYPE_OFFSET: usize = 64;
pub(super) const FO4_MGEF_ARCHETYPE_SCRIPT: u32 = 1;
pub(super) const FO4_MGEF_ARCHETYPE_STAGGER: u32 = 33;
pub(super) const FO76_MGEF_ARCHETYPE_PLAYER_FEAR: u32 = 20;
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
            // FO4's slot 20 is Telekinesis. Tales supplies the missing player-fear
            // behavior through the ScriptEffect lifecycle instead.
            FO76_MGEF_ARCHETYPE_PLAYER_FEAR => FO4_MGEF_ARCHETYPE_SCRIPT,
            FO76_MGEF_ARCHETYPE_STUN => FO4_MGEF_ARCHETYPE_STAGGER,
            FO76_MGEF_ARCHETYPE_TURBO_FERT | FO76_MGEF_ARCHETYPE_CORPSE_HIGHLIGHT => {
                FO4_MGEF_ARCHETYPE_SCRIPT
            }
            value if value > FO4_MAX_MGEF_ARCHETYPE => FO4_MGEF_ARCHETYPE_SCRIPT,
            _ => return,
        };
        chunk.copy_from_slice(&normalized.to_le_bytes());
    }

    pub(super) fn normalize_perk_entry_layout(record: &mut Record) {
        if record.sig.0 != *b"PERK" {
            return;
        }

        let mut entry_data_pending = false;
        for entry in &mut record.fields {
            match entry.sig.0 {
                sig if sig == *b"PRKE" => {
                    entry_data_pending = matches!(&entry.value,
                        FieldValue::Bytes(bytes) if bytes.first() == Some(&2));
                    if let FieldValue::Bytes(bytes) = &mut entry.value
                        && bytes.len() == 2
                    {
                        bytes.push(0);
                    }
                }
                sig if sig == *b"DATA" && entry_data_pending => {
                    if let FieldValue::Bytes(bytes) = &mut entry.value
                        && bytes.len() == 4
                    {
                        bytes.truncate(3);
                    }
                    entry_data_pending = false;
                }
                sig if sig == *b"PRKF" => entry_data_pending = false,
                _ => {}
            }
        }
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

    /// The FO76 lunchbox applies its reward MGEF twice: to self and as a 500-unit
    /// team area. FO4 lands both on the player, rolling two party favors, so
    /// only the self copy is kept.
    pub(super) fn drop_lunchbox_reward_area_effect(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"ALCH"
            || !interner
                .resolve(record.form_key.plugin)
                .is_some_and(|name| name.eq_ignore_ascii_case(FO76_MASTER_NAME))
        {
            return;
        }
        let is_reward = |entry: &FieldEntry| {
            entry.sig.0 == *b"EFID"
                && match &entry.value {
                    FieldValue::Bytes(bytes) if bytes.len() == 4 => {
                        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) & 0x00FF_FFFF
                            == LUNCHBOX_REWARD_MGEF_ID
                    }
                    FieldValue::FormKey(key) => {
                        key.local == LUNCHBOX_REWARD_MGEF_ID
                            && interner
                                .resolve(key.plugin)
                                .is_some_and(|name| name.eq_ignore_ascii_case(FO76_MASTER_NAME))
                    }
                    _ => false,
                }
        };
        let group_end = |fields: &[FieldEntry], start: usize| {
            fields[start + 1..]
                .iter()
                .position(|entry| !EFFECT_GROUP_CHILD_SIGS.contains(&entry.sig.0))
                .map_or(fields.len(), |offset| start + 1 + offset)
        };
        let has_area = |group: &[FieldEntry]| {
            group.iter().any(|entry| {
                entry.sig.0 == *b"EFIT"
                    && matches!(&entry.value, FieldValue::Bytes(bytes)
                if efit_area(bytes).is_some_and(|area| area > 0))
            })
        };

        let reward_groups: Vec<(usize, usize, bool)> = (0..record.fields.len())
            .filter(|&index| is_reward(&record.fields[index]))
            .map(|index| {
                let end = group_end(&record.fields, index);
                (index, end, has_area(&record.fields[index..end]))
            })
            .collect();
        if !reward_groups.iter().any(|&(_, _, area)| !area) {
            return;
        }
        for &(start, end, area) in reward_groups.iter().rev() {
            if area {
                record.fields.drain(start..end);
            }
        }
    }
}

const LUNCHBOX_REWARD_MGEF_ID: u32 = 0x3DF248;
const EFFECT_GROUP_CHILD_SIGS: [[u8; 4]; 8] = [
    *b"EFIT", *b"CTDA", *b"CIS1", *b"CIS2", *b"MAGF", *b"DURG", *b"MAGG", *b"CODV",
];

/// FO76 EFIT is `range, magnitude, area, duration`; FO4's drops the leading word.
fn efit_area(bytes: &[u8]) -> Option<u32> {
    let offset = match bytes.len() {
        16 => 8,
        12 => 4,
        _ => return None,
    };
    Some(u32::from_le_bytes(
        bytes[offset..offset + 4].try_into().ok()?,
    ))
}
