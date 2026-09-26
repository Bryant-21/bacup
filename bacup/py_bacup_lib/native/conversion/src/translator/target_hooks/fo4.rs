//! Fo4TargetHook: FO4-target record hook.
//!
//! `run()` applies these after the pair-level translation pass, each through a
//! private helper keyed on the record's `sig`:
//!
//! - IDLE: drop any residual `RELI` (RelatedIdles); the field-translation pass
//!   synthesizes `Animations`.
//! - NPC_: a bare boolean `DATA = true` becomes the FO4 struct form
//!   `DATA = {Marker: true}`. Strip `HasBaseSoundData`; restrict TemplateFlags
//!   to the FO4 subset (Traits, Stats, Factions, AIData, AIPackages,
//!   ModelAnimation, BaseData, Inventory, Script).
//! - BOOK: clear the FO76-only `IsRecipe` DNAM flag.
//! - ENCH: EffectData TargetType `Contact` (FO76) → `Touch` (FO4).
//! - ARMO / ARMA: normalize FO76-only BipedBodyTemplate slots to FO4 CK-valid
//!   slots; strip FO76-only sub-keys (XFLG, ENLT, ENLS, AUUV, MODD, ENLM) from
//!   each ARMA biped model entry.
//! - LVLI / LVLN: keep `LVLO` entries (`LeveledEntry` is a YAML-level alias,
//!   not a subrecord sig).
//! - LVLI: drop `CurveTablesMin`/`CurveTablesMax` from each COED entry.

use crate::ids::FormKey;
use crate::ids::SubrecordSig;
use crate::record::{FieldValue, Record};
use crate::sym::{StringInterner, Sym};
use crate::translator::pair_hook::HookResult;
use crate::translator::target_hook::{TargetCtx, TargetHook};

/// FO4-target hook.
pub struct Fo4TargetHook;

// ---------------------------------------------------------------------------
// Interned key caches — built on first use via `StringInterner::intern`.
// ---------------------------------------------------------------------------

/// String constants used as struct field keys in FieldValue::Struct entries.
mod keys {
    pub const HAS_BASE_SOUND_DATA: &str = "HasBaseSoundData";
    pub const TEMPLATE_FLAGS: &str = "TemplateFlags";
    pub const FLAGS: &str = "Flags";
    pub const IS_RECIPE: &str = "IsRecipe";
    pub const TARGET_TYPE: &str = "TargetType";
    pub const CONTACT: &str = "Contact";
    pub const TOUCH: &str = "Touch";
    pub const MARKER: &str = "Marker";
    pub const CURVE_TABLES_MIN: &str = "CurveTablesMin";
    pub const CURVE_TABLES_MAX: &str = "CurveTablesMax";
    pub const FIRST_PERSON_FLAGS: &str = "FirstPersonFlags";
    pub const RACE: &str = "Race";
    pub const MOD2: &str = "MOD2";
    pub const MOD3: &str = "MOD3";
    pub const MO2T: &str = "MO2T";
    pub const MO3T: &str = "MO3T";
    pub const MO2F: &str = "MO2F";
    pub const MO3F: &str = "MO3F";
    /// FO4-allowed TemplateFlags — mirrors the Python `allowed` set.
    pub const ALLOWED_TEMPLATE_FLAGS: &[&str] = &[
        "Traits",
        "Stats",
        "Factions",
        "AIData",
        "AIPackages",
        "ModelAnimation",
        "BaseData",
        "Inventory",
        "Script",
    ];

    /// FO76-only sub-keys to strip from ARMA BipedModels entries.
    pub const ARMA_DROP_KEYS: &[&str] = &["XFLG", "ENLT", "ENLS", "AUUV", "MODD", "ENLM"];
}

const FO76_INCOMPATIBLE_FACE_BONES_ARMA_EDITOR_IDS: &[&str] = &[
    "AAHeadwearLostHeadMirror_Storm",
    "AA_HeadwearSettlerWorkChief",
];

const BIPED_SLOT_33_BODY: u64 = 1 << (33 - 30);
const BIPED_SLOT_34_LEFT_HAND: u64 = 1 << (34 - 30);
const BIPED_SLOT_35_RIGHT_HAND: u64 = 1 << (35 - 30);
#[cfg(test)]
const BIPED_SLOT_54_BACKPACK: u64 = 1 << (54 - 30);
const BIPED_SLOT_55_EYE_OF_RA: u64 = 1 << (55 - 30);
const BIPED_SLOT_56_UNNAMED: u64 = 1 << (56 - 30);
const BIPED_SLOT_57_COVERALL: u64 = 1 << (57 - 30);
const BIPED_SLOT_58_UNNAMED: u64 = 1 << (58 - 30);
const BIPED_SLOT_60_PIPBOY: u64 = 1 << (60 - 30);
const BIPED_SLOT_61_FX: u64 = 1 << (61 - 30);

const FO76_HUMAN_ATTACHMENT_SLOTS_TO_DROP: u64 = BIPED_SLOT_55_EYE_OF_RA
    | BIPED_SLOT_56_UNNAMED
    | BIPED_SLOT_57_COVERALL
    | BIPED_SLOT_58_UNNAMED;
// ---------------------------------------------------------------------------
// Helper: look up a Sym inside a Struct's field list.
// ---------------------------------------------------------------------------

fn struct_find<'a>(fields: &'a [(Sym, FieldValue)], key: Sym) -> Option<&'a FieldValue> {
    fields.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
}

fn struct_find_mut<'a>(
    fields: &'a mut Vec<(Sym, FieldValue)>,
    key: Sym,
) -> Option<&'a mut FieldValue> {
    fields.iter_mut().find(|(k, _)| *k == key).map(|(_, v)| v)
}

fn race_formkey_from_record(record: &Record, interner: &StringInterner) -> Option<FormKey> {
    let rnam_sig = SubrecordSig(*b"RNAM");
    let race_sym = interner.intern(keys::RACE);

    record
        .fields
        .iter()
        .find(|entry| entry.sig == rnam_sig)
        .and_then(|entry| match &entry.value {
            FieldValue::FormKey(fk) => Some(*fk),
            FieldValue::Struct(fields) => struct_find(fields, race_sym).and_then(|value| {
                if let FieldValue::FormKey(fk) = value {
                    Some(*fk)
                } else {
                    None
                }
            }),
            _ => None,
        })
}

fn is_human_race_formkey(fk: Option<FormKey>, interner: &StringInterner) -> bool {
    let Some(fk) = fk else {
        return false;
    };
    let Some(plugin) = interner.resolve(fk.plugin) else {
        return false;
    };
    fk.local == 0x013746
        && (plugin.eq_ignore_ascii_case("Fallout4.esm")
            || plugin.eq_ignore_ascii_case("SeventySix.esm"))
}

fn is_human_hand_addon_formkey(fk: &FormKey, interner: &StringInterner) -> bool {
    let Some(plugin) = interner.resolve(fk.plugin) else {
        return false;
    };
    matches!(fk.local, 0x000D6C | 0x01D980 | 0x0316C7)
        && (plugin.eq_ignore_ascii_case("Fallout4.esm")
            || plugin.eq_ignore_ascii_case("SeventySix.esm"))
}

fn value_contains_human_hand_addon(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::FormKey(fk) => is_human_hand_addon_formkey(fk, interner),
        FieldValue::List(items) => items
            .iter()
            .any(|item| value_contains_human_hand_addon(item, interner)),
        FieldValue::Struct(fields) => fields
            .iter()
            .any(|(_, value)| value_contains_human_hand_addon(value, interner)),
        _ => false,
    }
}

fn record_has_human_hand_addon(record: &Record, interner: &StringInterner) -> bool {
    let modl_sig = SubrecordSig(*b"MODL");
    record
        .fields
        .iter()
        .filter(|entry| entry.sig == modl_sig)
        .any(|entry| value_contains_human_hand_addon(&entry.value, interner))
}

fn arma_has_actor_model(record: &Record, interner: &StringInterner) -> bool {
    let modl_sig = SubrecordSig(*b"MODL");
    let mod2_sym = interner.intern(keys::MOD2);
    let mod3_sym = interner.intern(keys::MOD3);

    for entry in record.fields.iter().filter(|entry| entry.sig == modl_sig) {
        let FieldValue::List(models) = &entry.value else {
            continue;
        };
        for model in models {
            let FieldValue::Struct(fields) = model else {
                continue;
            };
            for (key, value) in fields {
                if (*key == mod2_sym || *key == mod3_sym)
                    && matches!(value, FieldValue::String(path) if !interner.resolve(*path).unwrap_or("").is_empty())
                {
                    return true;
                }
            }
        }
    }

    false
}

fn normalize_fo76_arma_biped_mask(mask: u64, human_race: bool, preserve_empty_source: bool) -> u64 {
    let had_eye_of_ra_slot = mask & BIPED_SLOT_55_EYE_OF_RA != 0;
    let mut normalized = mask & !FO76_HUMAN_ATTACHMENT_SLOTS_TO_DROP;

    if had_eye_of_ra_slot {
        normalized |= BIPED_SLOT_61_FX;
    }
    if !human_race && normalized & BIPED_SLOT_60_PIPBOY != 0 {
        normalized &= !BIPED_SLOT_60_PIPBOY;
        normalized |= BIPED_SLOT_61_FX;
    }
    if normalized == 0 && !(preserve_empty_source && mask == 0) {
        normalized = BIPED_SLOT_33_BODY;
    }

    normalized
}

fn biped_slot_from_token(token: &str) -> Option<u8> {
    let digits: String = token.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<u8>().ok()
}

fn normalized_biped_token(slot: u8) -> Option<&'static str> {
    match slot {
        33 => Some("33BODY"),
        34 => Some("34LHand"),
        35 => Some("35RHand"),
        54 => Some("54Unnamed"),
        61 => Some("61FX"),
        _ => None,
    }
}

fn push_unique_token(
    interner: &StringInterner,
    output: &mut Vec<FieldValue>,
    seen_slots: &mut Vec<u8>,
    token: &str,
) {
    let slot = biped_slot_from_token(token);
    if let Some(slot) = slot {
        if seen_slots.contains(&slot) {
            return;
        }
        seen_slots.push(slot);
    }
    output.push(FieldValue::String(interner.intern(token)));
}

fn normalize_fo76_arma_biped_list(
    items: &mut Vec<FieldValue>,
    human_race: bool,
    preserve_empty_source: bool,
    interner: &StringInterner,
) {
    let input_was_empty = items.is_empty();
    let mut output = Vec::with_capacity(items.len().max(1));
    let mut seen_slots: Vec<u8> = Vec::new();

    for item in items.drain(..) {
        match item {
            FieldValue::String(sym) => {
                let token = interner.resolve(sym).unwrap_or("");
                match biped_slot_from_token(token) {
                    Some(54) => {
                        push_unique_token(interner, &mut output, &mut seen_slots, "54Unnamed");
                    }
                    Some(55) => {
                        push_unique_token(interner, &mut output, &mut seen_slots, "61FX");
                    }
                    Some(56) | Some(57) | Some(58) => {}
                    Some(60) if !human_race => {
                        push_unique_token(interner, &mut output, &mut seen_slots, "61FX");
                    }
                    Some(slot) => {
                        if seen_slots.contains(&slot) {
                            continue;
                        }
                        seen_slots.push(slot);
                        output.push(FieldValue::String(sym));
                    }
                    None => output.push(FieldValue::String(sym)),
                }
            }
            other => output.push(other),
        }
    }

    if output.is_empty() && !(preserve_empty_source && input_was_empty) {
        let fallback = normalized_biped_token(33).unwrap();
        push_unique_token(interner, &mut output, &mut seen_slots, fallback);
    }

    *items = output;
}

fn normalize_fo76_arma_biped_value(
    value: &mut FieldValue,
    human_race: bool,
    preserve_empty_source: bool,
    interner: &StringInterner,
) {
    match value {
        FieldValue::Uint(mask) => {
            *mask = normalize_fo76_arma_biped_mask(*mask, human_race, preserve_empty_source);
        }
        FieldValue::Int(mask) if *mask >= 0 => {
            let normalized =
                normalize_fo76_arma_biped_mask(*mask as u64, human_race, preserve_empty_source);
            *mask = normalized as i64;
        }
        FieldValue::List(items) => {
            normalize_fo76_arma_biped_list(items, human_race, preserve_empty_source, interner);
        }
        FieldValue::Struct(fields) => {
            let first_person_flags_sym = interner.intern(keys::FIRST_PERSON_FLAGS);
            if let Some(flags_value) = struct_find_mut(fields, first_person_flags_sym) {
                normalize_fo76_arma_biped_value(
                    flags_value,
                    human_race,
                    preserve_empty_source,
                    interner,
                );
            }
        }
        _ => {}
    }
}

fn biped_value_has_slot(value: &FieldValue, slot: u8, interner: &StringInterner) -> bool {
    let slot_bit = 1_u64 << (slot - 30);
    match value {
        FieldValue::Uint(mask) => *mask & slot_bit != 0,
        FieldValue::Int(mask) if *mask >= 0 => (*mask as u64) & slot_bit != 0,
        FieldValue::List(items) => items.iter().any(|item| {
            let FieldValue::String(sym) = item else {
                return false;
            };
            biped_slot_from_token(interner.resolve(*sym).unwrap_or("")) == Some(slot)
        }),
        FieldValue::Struct(fields) => {
            let first_person_flags_sym = interner.intern(keys::FIRST_PERSON_FLAGS);
            struct_find(fields, first_person_flags_sym)
                .is_some_and(|flags| biped_value_has_slot(flags, slot, interner))
        }
        _ => false,
    }
}

fn ensure_human_hand_slots(value: &mut FieldValue, interner: &StringInterner) {
    match value {
        FieldValue::Uint(mask) => {
            *mask |= BIPED_SLOT_34_LEFT_HAND | BIPED_SLOT_35_RIGHT_HAND;
        }
        FieldValue::Int(mask) if *mask >= 0 => {
            *mask |= (BIPED_SLOT_34_LEFT_HAND | BIPED_SLOT_35_RIGHT_HAND) as i64;
        }
        FieldValue::List(items) => {
            let mut seen_slots = items
                .iter()
                .filter_map(|item| {
                    let FieldValue::String(sym) = item else {
                        return None;
                    };
                    biped_slot_from_token(interner.resolve(*sym).unwrap_or(""))
                })
                .collect::<Vec<_>>();
            push_unique_token(interner, items, &mut seen_slots, "34LHand");
            push_unique_token(interner, items, &mut seen_slots, "35RHand");
        }
        FieldValue::Struct(fields) => {
            let first_person_flags_sym = interner.intern(keys::FIRST_PERSON_FLAGS);
            if let Some(flags_value) = struct_find_mut(fields, first_person_flags_sym) {
                ensure_human_hand_slots(flags_value, interner);
            }
        }
        _ => {}
    }
}

fn ensure_arma_female_model_from_male(record: &mut Record, interner: &StringInterner) {
    let modl_sig = SubrecordSig(*b"MODL");
    let mod2_sym = interner.intern(keys::MOD2);
    let mod3_sym = interner.intern(keys::MOD3);
    let mo2t_sym = interner.intern(keys::MO2T);
    let mo3t_sym = interner.intern(keys::MO3T);

    for entry in record.fields.iter_mut() {
        if entry.sig != modl_sig {
            continue;
        }
        if let FieldValue::List(models) = &mut entry.value {
            for model in models.iter_mut() {
                let FieldValue::Struct(fields) = model else {
                    continue;
                };
                let male_model = fields
                    .iter()
                    .find(|(key, _)| *key == mod2_sym)
                    .map(|(_, value)| value.clone());
                if !fields.iter().any(|(key, _)| *key == mod3_sym) {
                    if let Some(value) = male_model {
                        fields.push((mod3_sym, value));
                    }
                }

                let male_texture = fields
                    .iter()
                    .find(|(key, _)| *key == mo2t_sym)
                    .map(|(_, value)| value.clone());
                if !fields.iter().any(|(key, _)| *key == mo3t_sym) {
                    if let Some(value) = male_texture {
                        fields.push((mo3t_sym, value));
                    }
                }
            }
        }
        break;
    }
}

// ---------------------------------------------------------------------------
// Per-sig behaviors
// ---------------------------------------------------------------------------

/// IDLE: drop any residual `RELI` (RelatedIdles) subrecord. The field-translation
/// pass synthesizes `Animations`; this drops any RELI it didn't consume.
fn apply_idle(record: &mut Record) {
    let reli = SubrecordSig(*b"RELI");
    record.fields.retain(|e| e.sig != reli);
}

/// NPC_: normalize DATA bool → struct marker; strip Configuration flags.
fn apply_npc(record: &mut Record, interner: &StringInterner) {
    // DATA true → {Marker: true}
    let data_sig = SubrecordSig(*b"DATA");
    for entry in record.fields.iter_mut() {
        if entry.sig == data_sig {
            if entry.value == FieldValue::Bool(true) {
                let marker_key = interner.intern(keys::MARKER);
                entry.value = FieldValue::Struct(vec![(marker_key, FieldValue::Bool(true))]);
            }
            break;
        }
    }

    // Configuration: strip HasBaseSoundData + restrict TemplateFlags allowlist
    let cfg_sig = SubrecordSig(*b"CNFG");
    let hbsd_sym = interner.intern(keys::HAS_BASE_SOUND_DATA);
    let flags_sym = interner.intern(keys::FLAGS);
    let tf_sym = interner.intern(keys::TEMPLATE_FLAGS);
    let allowed_syms: Vec<Sym> = keys::ALLOWED_TEMPLATE_FLAGS
        .iter()
        .map(|s| interner.intern(s))
        .collect();

    for entry in record.fields.iter_mut() {
        if entry.sig != cfg_sig {
            continue;
        }
        if let FieldValue::Struct(fields) = &mut entry.value {
            // Strip HasBaseSoundData from Flags list
            if let Some(flags_val) = struct_find_mut(fields, flags_sym) {
                if let FieldValue::List(list) = flags_val {
                    list.retain(|v| {
                        if let FieldValue::String(s) = v {
                            *s != hbsd_sym
                        } else {
                            true
                        }
                    });
                }
            }
            // Restrict TemplateFlags to allowed set
            if let Some(tf_val) = struct_find_mut(fields, tf_sym) {
                if let FieldValue::List(list) = tf_val {
                    list.retain(|v| {
                        if let FieldValue::String(s) = v {
                            allowed_syms.contains(s)
                        } else {
                            false
                        }
                    });
                }
            }
        }
        break;
    }
}

/// BOOK: remove `IsRecipe` from DNAM Flags list.
fn apply_book(record: &mut Record, interner: &StringInterner) {
    let dnam_sig = SubrecordSig(*b"DNAM");
    let flags_sym = interner.intern(keys::FLAGS);
    let is_recipe_sym = interner.intern(keys::IS_RECIPE);

    for entry in record.fields.iter_mut() {
        if entry.sig != dnam_sig {
            continue;
        }
        if let FieldValue::Struct(fields) = &mut entry.value {
            if let Some(flags_val) = struct_find_mut(fields, flags_sym) {
                if let FieldValue::List(list) = flags_val {
                    list.retain(|v| !matches!(v, FieldValue::String(s) if *s == is_recipe_sym));
                }
            }
        }
        break;
    }
}

/// ENCH: EffectData TargetType "Contact" → "Touch".
fn apply_ench(record: &mut Record, interner: &StringInterner) {
    // EffectData is stored as the EITM subrecord or under a named field.
    // In the Rust model it will appear as a subrecord; we look for the
    // TargetType key inside any Struct-valued subrecord.
    let tt_sym = interner.intern(keys::TARGET_TYPE);
    let contact_sym = interner.intern(keys::CONTACT);
    let touch_sym = interner.intern(keys::TOUCH);

    for entry in record.fields.iter_mut() {
        if let FieldValue::Struct(fields) = &mut entry.value {
            if let Some(tt_val) = struct_find_mut(fields, tt_sym) {
                if *tt_val == FieldValue::String(contact_sym) {
                    *tt_val = FieldValue::String(touch_sym);
                }
            }
        }
    }
}

/// ARMO: normalize FO76-only BipedBodyTemplate slots.
fn apply_armo(record: &mut Record, interner: &StringInterner) {
    let race = race_formkey_from_record(record, interner);
    let human_race = is_human_race_formkey(race, interner);
    let preserve_empty_source = race.is_some() && !human_race;
    let has_human_hand_addon = human_race && record_has_human_hand_addon(record, interner);

    let bod2_sig = SubrecordSig(*b"BOD2");
    for entry in record.fields.iter_mut() {
        if entry.sig != bod2_sig {
            continue;
        }
        normalize_fo76_arma_biped_value(
            &mut entry.value,
            human_race,
            preserve_empty_source,
            interner,
        );
        if has_human_hand_addon {
            ensure_human_hand_slots(&mut entry.value, interner);
        }
        break;
    }
}

/// ARMA: normalize FO76-only BipedBodyTemplate slots and strip FO76-only keys
/// from BipedModels struct entries.
fn apply_arma(record: &mut Record, interner: &StringInterner) {
    let race = race_formkey_from_record(record, interner);
    let human_race = is_human_race_formkey(race, interner);
    let has_actor_model = arma_has_actor_model(record, interner);
    let mut has_backpack_slot = false;

    let bod2_sig = SubrecordSig(*b"BOD2");
    for entry in record.fields.iter_mut() {
        if entry.sig != bod2_sig {
            continue;
        }
        has_backpack_slot = biped_value_has_slot(&entry.value, 54, interner);
        normalize_fo76_arma_biped_value(&mut entry.value, human_race, !has_actor_model, interner);
        break;
    }

    let mut drop_keys = keys::ARMA_DROP_KEYS.to_vec();
    if record
        .eid
        .and_then(|eid| interner.resolve(eid))
        .is_some_and(|eid| FO76_INCOMPATIBLE_FACE_BONES_ARMA_EDITOR_IDS.contains(&eid))
    {
        drop_keys.extend([keys::MO2F, keys::MO3F]);
    }

    let drop_sigs: Vec<SubrecordSig> = drop_keys
        .iter()
        .map(|key| SubrecordSig::from_str(key).expect("ARMA drop key must be four bytes"))
        .collect();
    record
        .fields
        .retain(|entry| !drop_sigs.contains(&entry.sig));

    // Structured fixtures and projected records may hold the same row as a list.
    let biped_models_sig = SubrecordSig(*b"MOD2");
    let drop_syms: Vec<Sym> = drop_keys.iter().map(|key| interner.intern(key)).collect();

    for entry in record.fields.iter_mut() {
        if entry.sig != biped_models_sig {
            continue;
        }
        if let FieldValue::List(models) = &mut entry.value {
            for model in models.iter_mut() {
                if let FieldValue::Struct(fields) = model {
                    fields.retain(|(k, _)| !drop_syms.contains(k));
                }
            }
        }
        break;
    }

    if human_race && has_backpack_slot {
        ensure_arma_female_model_from_male(record, interner);
    }
}

/// LVLI / LVLN: keep binary LVLO entries intact.
fn apply_leveled_list(_record: &mut Record) {}

/// LVLI: strip CurveTablesMin / CurveTablesMax from COED struct entries.
fn apply_lvli_coed(record: &mut Record, interner: &StringInterner) {
    let coed_sig = SubrecordSig(*b"COED");
    let ctmin_sym = interner.intern(keys::CURVE_TABLES_MIN);
    let ctmax_sym = interner.intern(keys::CURVE_TABLES_MAX);

    for entry in record.fields.iter_mut() {
        if entry.sig != coed_sig {
            continue;
        }
        let strip = |fields: &mut Vec<(Sym, FieldValue)>| {
            fields.retain(|(k, _)| *k != ctmin_sym && *k != ctmax_sym);
        };
        match &mut entry.value {
            FieldValue::Struct(fields) => strip(fields),
            FieldValue::List(list) => {
                for item in list.iter_mut() {
                    if let FieldValue::Struct(fields) = item {
                        strip(fields);
                    }
                }
            }
            _ => {}
        }
        // There may be multiple COED entries; continue iterating.
    }
}

// ---------------------------------------------------------------------------
// TargetHook impl
// ---------------------------------------------------------------------------

impl TargetHook for Fo4TargetHook {
    fn run(&self, ctx: &mut TargetCtx<'_>, record: &mut Record) -> HookResult {
        match &record.sig.0 {
            b"IDLE" => apply_idle(record),
            b"NPC_" => apply_npc(record, ctx.interner),
            b"BOOK" => apply_book(record, ctx.interner),
            b"ENCH" => apply_ench(record, ctx.interner),
            b"ARMO" => apply_armo(record, ctx.interner),
            b"ARMA" => apply_arma(record, ctx.interner),
            b"LVLI" => {
                apply_leveled_list(record);
                apply_lvli_coed(record, ctx.interner);
            }
            b"LVLN" => apply_leveled_list(record),
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record};
    use crate::sym::StringInterner;
    use crate::translator::target_hook::TargetCtx;

    fn make_record(sig: &str, interner: &StringInterner) -> Record {
        let fk = FormKey::parse("000800@SeventySix.esm", interner).unwrap();
        Record::new(SigCode::from_str(sig).unwrap(), fk)
    }

    fn push_field(record: &mut Record, sig: &str, value: FieldValue) {
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        });
    }

    fn push_rnam(record: &mut Record, local: u32, plugin: &str, interner: &StringInterner) {
        push_field(
            record,
            "RNAM",
            FieldValue::FormKey(FormKey {
                local,
                plugin: interner.intern(plugin),
            }),
        );
    }

    fn bod2_mask(record: &Record) -> u64 {
        let bod2 = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "BOD2")
            .expect("BOD2 should exist");
        match bod2.value {
            FieldValue::Uint(mask) => mask,
            ref other => panic!("expected Uint BOD2, got {other:?}"),
        }
    }

    fn bod2_list_tokens(record: &Record, interner: &StringInterner) -> Vec<String> {
        let bod2 = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "BOD2")
            .expect("BOD2 should exist");
        let FieldValue::List(items) = &bod2.value else {
            panic!("expected List BOD2, got {:?}", bod2.value);
        };
        items
            .iter()
            .map(|item| {
                let FieldValue::String(sym) = item else {
                    panic!("expected string biped token, got {item:?}");
                };
                interner.resolve(*sym).unwrap().to_string()
            })
            .collect()
    }

    fn run(record: &mut Record, interner: &StringInterner) {
        Fo4TargetHook
            .run(&mut TargetCtx { interner }, record)
            .unwrap();
    }

    fn sigs(record: &Record) -> Vec<&str> {
        record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect()
    }

    fn flag_list(record: &Record, key: Sym) -> &Vec<FieldValue> {
        let FieldValue::Struct(fields) = &record.fields[0].value else {
            panic!("expected Struct, got {:?}", record.fields[0].value);
        };
        let Some(FieldValue::List(list)) = struct_find(fields, key) else {
            panic!("expected List flags");
        };
        list
    }

    fn push_mesh_model(record: &mut Record, interner: &StringInterner) {
        push_field(
            record,
            "MODL",
            FieldValue::List(vec![FieldValue::Struct(vec![(
                interner.intern("MOD2"),
                FieldValue::String(interner.intern("mesh.nif")),
            )])]),
        );
    }

    #[test]
    fn idle_drops_reli_subrecord() {
        let interner = StringInterner::new();
        let mut record = make_record("IDLE", &interner);
        push_field(&mut record, "RELI", FieldValue::None);
        push_field(&mut record, "EDID", FieldValue::None);
        run(&mut record, &interner);
        assert_eq!(sigs(&record), vec!["EDID"]);

        let mut record = make_record("IDLE", &interner);
        push_field(&mut record, "EDID", FieldValue::None);
        run(&mut record, &interner);
        assert_eq!(sigs(&record), vec!["EDID"]);
    }

    #[test]
    fn npc_data_bool_true_becomes_marker_struct() {
        let interner = StringInterner::new();
        let mut record = make_record("NPC_", &interner);
        push_field(&mut record, "DATA", FieldValue::Bool(true));
        run(&mut record, &interner);
        assert_eq!(
            record.fields[0].value,
            FieldValue::Struct(vec![(interner.intern("Marker"), FieldValue::Bool(true))])
        );

        let mut record = make_record("NPC_", &interner);
        push_field(&mut record, "DATA", FieldValue::Bool(false));
        run(&mut record, &interner);
        assert_eq!(record.fields[0].value, FieldValue::Bool(false));
    }

    #[test]
    fn npc_configuration_strips_fo76_only_flags() {
        let interner = StringInterner::new();
        for (list_key, kept, removed) in [
            ("Flags", "Unique", "HasBaseSoundData"),
            ("TemplateFlags", "Traits", "Spells"),
        ] {
            let key = interner.intern(list_key);
            let kept = FieldValue::String(interner.intern(kept));
            let removed = FieldValue::String(interner.intern(removed));
            let mut record = make_record("NPC_", &interner);
            push_field(
                &mut record,
                "CNFG",
                FieldValue::Struct(vec![(
                    key,
                    FieldValue::List(vec![removed.clone(), kept.clone()]),
                )]),
            );
            run(&mut record, &interner);
            assert_eq!(flag_list(&record, key), &vec![kept], "{list_key}");
        }
    }

    #[test]
    fn book_dnam_removes_is_recipe_flag() {
        let interner = StringInterner::new();
        let flags_sym = interner.intern("Flags");
        let teach = FieldValue::String(interner.intern("TeachesSpell"));
        let mut record = make_record("BOOK", &interner);
        push_field(
            &mut record,
            "DNAM",
            FieldValue::Struct(vec![(
                flags_sym,
                FieldValue::List(vec![
                    FieldValue::String(interner.intern("IsRecipe")),
                    teach.clone(),
                ]),
            )]),
        );
        run(&mut record, &interner);
        assert_eq!(flag_list(&record, flags_sym), &vec![teach]);
    }

    #[test]
    fn ench_renames_contact_to_touch() {
        let interner = StringInterner::new();
        let tt_sym = interner.intern("TargetType");
        for (input, expected) in [("Contact", "Touch"), ("Self", "Self")] {
            let mut record = make_record("ENCH", &interner);
            push_field(
                &mut record,
                "EITM",
                FieldValue::Struct(vec![(tt_sym, FieldValue::String(interner.intern(input)))]),
            );
            run(&mut record, &interner);
            let FieldValue::Struct(fields) = &record.fields[0].value else {
                panic!("expected Struct for EITM");
            };
            assert_eq!(
                struct_find(fields, tt_sym),
                Some(&FieldValue::String(interner.intern(expected))),
                "{input}"
            );
        }
    }

    #[test]
    fn armo_normalizes_fo76_biped_template_slots() {
        let human = Some((0x013746, "Fallout4.esm"));
        let nonhuman = Some((0x6356DD, "SeventySix.esm"));
        let creature = Some((0x822A4D, "SeventySix.esm"));
        let hands: &[&str] = &["33BODY", "34LHand", "35RHand"];
        let cases: &[(&str, Option<(u32, &str)>, &[&str], Option<u32>, &[&str])] = &[
            ("coverall", human, &["57Coverall"], None, &["33BODY"]),
            ("naked hands", human, &["57Coverall"], Some(0x000D6C), hands),
            (
                "raider gloves",
                human,
                &["57Coverall"],
                Some(0x01D980),
                hands,
            ),
            (
                "preston gloves",
                human,
                &["57Coverall"],
                Some(0x0316C7),
                hands,
            ),
            ("backpack", human, &["54Backpack"], None, &["54Unnamed"]),
            (
                "nonhuman pipboy",
                nonhuman,
                &["33BODY", "60Pipboy", "61FX"],
                None,
                &["33BODY", "61FX"],
            ),
            ("empty", None, &[], None, &["33BODY"]),
            ("empty creature", creature, &[], None, &[]),
        ];
        for (name, race, tokens, addon, expected) in cases {
            let interner = StringInterner::new();
            let mut record = make_record("ARMO", &interner);
            if let Some((local, plugin)) = race {
                push_rnam(&mut record, *local, plugin, &interner);
            }
            push_field(
                &mut record,
                "BOD2",
                FieldValue::List(
                    tokens
                        .iter()
                        .map(|token| FieldValue::String(interner.intern(token)))
                        .collect(),
                ),
            );
            if let Some(local) = addon {
                push_field(
                    &mut record,
                    "MODL",
                    FieldValue::FormKey(FormKey {
                        local: *local,
                        plugin: interner.intern("Fallout4.esm"),
                    }),
                );
            }
            run(&mut record, &interner);
            assert_eq!(bod2_list_tokens(&record, &interner), *expected, "{name}");
        }
    }

    #[test]
    fn arma_strips_fo76_biped_model_keys_and_face_bone_flags() {
        let interner = StringInterner::new();
        let mut record = make_record("ARMA", &interner);
        push_field(
            &mut record,
            "MOD2",
            FieldValue::String(interner.intern("mesh.nif")),
        );
        push_field(&mut record, "XFLG", FieldValue::None);
        push_field(&mut record, "ENLT", FieldValue::None);
        run(&mut record, &interner);
        assert_eq!(sigs(&record), vec!["MOD2"]);

        let has_face_bones = interner.intern("HasFaceBonesModel");
        for (eid, models) in [
            (
                "AAHeadwearLostHeadMirror_Storm",
                &[
                    ("MOD2", "MO2F", "clothes/LostHeadMirror/HeadMirrorM.nif"),
                    ("MOD3", "MO3F", "clothes/LostHeadMirror/HeadMirrorF.nif"),
                ][..],
            ),
            (
                "AA_HeadwearSettlerWorkChief",
                &[(
                    "MOD2",
                    "MO2F",
                    "clothes/Settler15_WorkChief/Settler15_Workchief_Hat_M.nif",
                )][..],
            ),
        ] {
            let mut record = make_record("ARMA", &interner);
            record.eid = Some(interner.intern(eid));
            for (model, flags, path) in models {
                push_field(
                    &mut record,
                    model,
                    FieldValue::String(interner.intern(path)),
                );
                push_field(
                    &mut record,
                    flags,
                    FieldValue::List(vec![FieldValue::String(has_face_bones)]),
                );
            }
            run(&mut record, &interner);
            let expected: Vec<&str> = models.iter().map(|(model, _, _)| *model).collect();
            assert_eq!(sigs(&record), expected, "{eid}");
        }
    }

    #[test]
    fn arma_normalizes_fo76_biped_template_slots() {
        let human = Some((0x013746, "Fallout4.esm"));
        let nonhuman = Some((0x6356DD, "SeventySix.esm"));
        for (name, race, mask, with_mesh, expected) in [
            (
                "coverall",
                human,
                BIPED_SLOT_33_BODY | BIPED_SLOT_57_COVERALL | BIPED_SLOT_60_PIPBOY,
                false,
                BIPED_SLOT_33_BODY | BIPED_SLOT_60_PIPBOY,
            ),
            (
                "backpack",
                human,
                BIPED_SLOT_54_BACKPACK,
                false,
                BIPED_SLOT_54_BACKPACK,
            ),
            (
                "nonhuman pipboy",
                nonhuman,
                BIPED_SLOT_60_PIPBOY,
                false,
                BIPED_SLOT_61_FX,
            ),
            ("empty with mesh", None, 0, true, BIPED_SLOT_33_BODY),
            ("empty without mesh", human, 0, false, 0),
        ] {
            let interner = StringInterner::new();
            let mut record = make_record("ARMA", &interner);
            if let Some((local, plugin)) = race {
                push_rnam(&mut record, local, plugin, &interner);
            }
            push_field(&mut record, "BOD2", FieldValue::Uint(mask));
            if with_mesh {
                push_mesh_model(&mut record, &interner);
            }
            run(&mut record, &interner);
            assert_eq!(bod2_mask(&record), expected, "{name}");
        }

        let interner = StringInterner::new();
        let mut record = make_record("ARMA", &interner);
        push_rnam(&mut record, 0x013746, "Fallout4.esm", &interner);
        push_field(
            &mut record,
            "BOD2",
            FieldValue::List(vec![
                FieldValue::String(interner.intern("33BODY")),
                FieldValue::String(interner.intern("57Coverall")),
                FieldValue::String(interner.intern("54Backpack")),
            ]),
        );
        run(&mut record, &interner);
        assert_eq!(
            bod2_list_tokens(&record, &interner),
            vec!["33BODY", "54Unnamed"]
        );

        let mut record = make_record("ARMA", &interner);
        push_field(&mut record, "BOD2", FieldValue::List(vec![]));
        push_mesh_model(&mut record, &interner);
        run(&mut record, &interner);
        assert_eq!(bod2_list_tokens(&record, &interner), vec!["33BODY"]);
    }

    #[test]
    fn arma_backpack_fills_missing_female_model_from_male_model() {
        let interner = StringInterner::new();
        let mod2_sym = interner.intern("MOD2");
        let mo2t_sym = interner.intern("MO2T");
        let mod3_sym = interner.intern("MOD3");
        let mo3t_sym = interner.intern("MO3T");
        let male_path = interner.intern("BackPacks/Backpack03/Backpack_03_M.nif");
        let texture_payload = FieldValue::Bytes(smallvec::SmallVec::from_slice(&[4, 0, 0, 0]));

        let model_entry = FieldValue::Struct(vec![
            (mod2_sym, FieldValue::String(male_path)),
            (mo2t_sym, texture_payload.clone()),
        ]);
        let mut record = make_record("ARMA", &interner);
        push_rnam(&mut record, 0x013746, "Fallout4.esm", &interner);
        push_field(
            &mut record,
            "BOD2",
            FieldValue::List(vec![FieldValue::String(interner.intern("54Backpack"))]),
        );
        push_field(&mut record, "MODL", FieldValue::List(vec![model_entry]));
        run(&mut record, &interner);

        let model_fields = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "MODL")
            .and_then(|entry| match &entry.value {
                FieldValue::List(models) => models.first(),
                _ => None,
            })
            .and_then(|model| match model {
                FieldValue::Struct(fields) => Some(fields),
                _ => None,
            })
            .unwrap();

        assert_eq!(
            struct_find(model_fields, mod3_sym),
            Some(&FieldValue::String(male_path))
        );
        assert_eq!(struct_find(model_fields, mo3t_sym), Some(&texture_payload));
    }

    #[test]
    fn leveled_lists_preserve_lvlo_and_unhandled_records_are_untouched() {
        let interner = StringInterner::new();
        for sig in ["LVLI", "LVLN", "WEAP"] {
            let mut record = make_record(sig, &interner);
            push_field(&mut record, "LVLO", FieldValue::None);
            push_field(&mut record, "EDID", FieldValue::None);
            run(&mut record, &interner);
            assert_eq!(sigs(&record), vec!["LVLO", "EDID"], "{sig}");
        }
    }

    #[test]
    fn lvli_coed_drops_curve_table_fields() {
        let interner = StringInterner::new();
        let ctmin_sym = interner.intern("CurveTablesMin");
        let ctmax_sym = interner.intern("CurveTablesMax");
        let owner_sym = interner.intern("Owner");

        let coed = FieldValue::Struct(vec![
            (owner_sym, FieldValue::None),
            (ctmin_sym, FieldValue::None),
            (ctmax_sym, FieldValue::None),
        ]);
        let mut record = make_record("LVLI", &interner);
        push_field(&mut record, "COED", coed);
        run(&mut record, &interner);

        let coed_entry = record
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "COED")
            .expect("COED should still exist");
        let FieldValue::Struct(ref fields) = coed_entry.value else {
            panic!("expected Struct for COED");
        };
        let keys: Vec<Sym> = fields.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, vec![owner_sym]);
    }
}
