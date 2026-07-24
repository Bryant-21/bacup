//! Fixup: strip / null references whose resolved target record type is illegal
//! for the FO4 field (Class C — reference type-mismatch).
//!
//! # Why
//! FO76 and FO4 disagree on the allowed target type of several FormID fields.
//! After the conversion remaps a reference, the FK resolves fine but points at
//! a record whose *signature* is not in the FO4 field's `wbFormIDCk` allow-set,
//! e.g. `FACT \ VENC -> Found a REFR? ...` or `DIAL \ BNAM -> non-DLBR`. xEdit
//! reports these as "Found a X reference, expected: Y".
//!
//! XEZN→LCTN (the dominant Class C case) is handled separately in
//! `rewrite_raw_object_template_formids::rewrite_placed_ref_location_record`;
//! this pass covers the remaining tail.
//!
//! # How
//! For each `(record_sig, field_path)` in `CLASS_C_TARGETS`, this pass walks the
//! matching subrecord of every output record of that sig, resolves the FK's
//! target record signature via the output FK→sig index, and consults the shared
//! schema accessor `CompiledSchema::allowed_targets(record_sig, field_path)`:
//! when the resolved sig is not allowed, the field is acted on per `Action`:
//!   - `Null`  — zero the FK in place, keep the subrecord (NULL is xEdit-legal).
//!   - `Strip` — remove the whole subrecord (single optional FK subrecord).
//!
//! A FK that does not resolve to any output/master record (dangling) is left
//! alone — `fix_invalid_target_formkeys` / `sweep_unmapped_formkeys` own that.
//! Only references whose target type is *positively known and wrong* are acted
//! on, mirroring the conservative policy in `fix_stag_sound_refs`.

use rustc_hash::{FxHashMap, FxHashSet};

use esp_authoring_core::plugin_runtime::compiled_schema_for_game;

use crate::fixups::ref_index::{build_target_fk_sig_map, validate_struct_fk_fields};
use crate::fixups::remap_struct_internal_formids::union_type_holds_formid;
use crate::fixups::rewrite_raw_object_template_formids::{
    encode_target_form_id, target_record_sigs_by_encoded_form_id,
};
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
use crate::schema::AuthoringSchema;
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};

// ---------------------------------------------------------------------------
// Class C tail target table
// ---------------------------------------------------------------------------

/// What to do when a field's resolved target type is illegal. Derived from
/// schema metadata (`null_allowed` + `subrecord_required`), NOT hardcoded —
/// see `action_for`. xEdit `wbFormIDCkNoReach` does NOT imply NULL is allowed,
/// so nulling a NULL-disallowed field would itself be an error; such optional
/// subrecords must be stripped, and required ones left (only a retarget fixes).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Action {
    /// Zero the FK in place (NULL is in the field's allowed-target set).
    Null,
    /// Remove the entire subrecord (NULL not allowed + subrecord optional).
    Strip,
    /// Leave as-is + warn (NULL not allowed + subrecord required: only a
    /// retarget would fix it; nulling/stripping would just move the error).
    Leave,
}

/// Derive the action for an illegal-type subrecord-level FK from schema
/// metadata. `null_allowed` = NULL ∈ the field's `wbFormIDCk` target set;
/// `required` = the subrecord is `.SetRequired` in the FO4 schema.
fn action_for(null_allowed: bool, required: bool) -> Action {
    if null_allowed {
        Action::Null
    } else if !required {
        Action::Strip
    } else {
        Action::Leave
    }
}

/// Subrecord-level Class C tail: `(record_sig, subrecord_sig)`. The action is
/// DERIVED per record from schema metadata (`action_for`), never hardcoded.
///
/// `field_path` passed to `allowed_targets` is the subrecord sig — every entry
/// here is a single-FK subrecord, so the schema's subrecord-level / sole-FK-field
/// fallback resolves the target spec. NOT listed here:
/// - XEZN (placed-ref encounter zone) → `rewrite_placed_ref_location_record`.
/// - MGEF DATA, RACE DATA, IDLE ANAM, etc. → struct-INTERNAL FKs, handled by the
///   struct-field pass (`validate_struct_fk_fields`) below.
const CLASS_C_TARGETS: &[(&str, &str)] = &[
    // DIAL BNAM → DLBR (NULL NOT in set → strip, optional subrecord).
    ("DIAL", "BNAM"),
    // DIAL KNAM → KYWD (NULL NOT in set → strip). A present-but-null KNAM
    // (upstream-nulled keyword) is "Found NULL, expected KYWD" in xEdit; the
    // Strip path drops the optional subrecord so it's absent (valid).
    ("DIAL", "KNAM"),
    // FACT VENC → REFR "Merchant Container" (NULL NOT in set → strip).
    ("FACT", "VENC"),
    // WRLD NAM2 / NAM3 → WATR water-type (NULL NOT in set → strip).
    ("WRLD", "NAM2"),
    ("WRLD", "NAM3"),
    // COBJ CNAM → created-object (NULL IS allowed → null). FO76 COBJs point at
    // PKIN (placed-instance kit) and other FO76-only types FO4 can't create; the
    // created-object FK is nullable, so null the wrong-type rather than strip the
    // whole COBJ.
    ("COBJ", "CNAM"),
    // REFR XRFG → RFGP (Reference Group) and XLYR → LAYR (Layer): FO76
    // editor-grouping refs with no FO4 home. RFGP/LAYR are not carried into the
    // exterior-only port, so the blind object-id remap either collided with an
    // unrelated 07 record (xEdit "Found a REFR/INFO/LVLI") or dangles at an
    // 01xxxxxx master index where no such record exists. Both are optional,
    // NULL-disallowed single-FK subrecords → Strip. Because the target type has
    // NO valid FO4 representation, the Strip action here also drops the dangling
    // (unresolved-non-null) variant — see `apply_to_record`'s strip-dangling path.
    ("REFR", "XRFG"),
    ("REFR", "XLYR"),
    // REFR XASP → REFR (Acoustic Parent): the FO76 acoustic-parent REFR lived in
    // a dropped interior, so the blind object-id remap collided with an unrelated
    // output record (xEdit "Found a LAND reference, expected: REFR"). NULL-
    // disallowed optional single-FK subrecord → Strip the wrong-type. (Pure
    // dangling XASP is the null-sweep's; only positively-wrong-type is stripped
    // here, so the slot's valid REFR target type is respected.)
    ("REFR", "XASP"),
];

/// Subrecords whose ONLY valid FO4 target record type is never carried into the
/// exterior-only APPALACHIA port (FO76 editor-grouping records: RFGP Reference
/// Group, LAYR Layer). For these, a non-null FK that resolves NOWHERE (output or
/// master) is just as wrong as a wrong-typed one — the slot can't point at a
/// valid record, so a `Strip` action drops the dangling variant too rather than
/// leaving it for the (correctly conservative) sweep fixups, which would keep it.
const STRIP_DANGLING_SUBRECORDS: &[(&str, &str)] = &[("REFR", "XRFG"), ("REFR", "XLYR")];

/// Keyword-LIST (formid_array) Class C targets: `(record_sig, subrecord_sig)`.
/// xEdit reports a wrong-type ENTRY (e.g. `KWDA -> Found a STAT reference,
/// expected: KYWD,NULL`) when a FO76 keyword's object-id, remapped to 07,
/// collided with an unrelated output record. NULL is allowed in these slots, so
/// the fix FILTERS the wrong-type entries and keeps the valid keywords — never
/// strips the whole list (which would drop legitimate keywords). Mirrors the
/// existing OMOD MNAM handling, generalised to the full set.
const KEYWORD_LIST_TARGETS: &[(&str, &str)] = &[
    ("OMOD", "MNAM"),
    ("FURN", "KWDA"),
    ("WEAP", "KWDA"),
    ("NPC_", "KWDA"),
];

/// Record signatures carrying struct-INTERNAL FK fields that need per-field
/// validation via `struct_field_layout` (the keystone util). These FKs live in
/// raw `struct:`-codec subrecord bytes the decoded-FieldValue walk can't reach.
const STRUCT_FK_SIGS: &[&str] = &["RACE", "MGEF", "IDLE", "COBJ", "LVLI"];

/// How a bytes-union FK's resolved target sig is judged wrong-type. The union
/// field carries no schema `formlink_targets` (so `allowed_targets` returns None),
/// so the allow/deny set is given explicitly here.
#[derive(Clone, Copy)]
enum UnionTypeRule {
    /// FK is wrong-type unless its resolved sig is in this allow-list.
    AllowOnly(&'static [&'static str]),
    /// FK is wrong-type when its resolved sig is in this deny-list. Used for the
    /// variant-dependent PACK target/location unions where the full per-variant
    /// allow-set is large but a handful of structural types (LAND/CELL/DIAL/…)
    /// are NEVER valid in any variant — denying those is conservative (never
    /// strips a legitimately-placed ref) and covers every observed wrong-type.
    Deny(&'static [&'static str]),
}

impl UnionTypeRule {
    fn is_wrong_type(&self, sig: &str) -> bool {
        match self {
            UnionTypeRule::AllowOnly(allow) => !allow.contains(&sig),
            UnionTypeRule::Deny(deny) => deny.contains(&sig),
        }
    }
}

/// What to do with a wrong-type bytes-union FK.
#[derive(Clone, Copy, PartialEq, Eq)]
enum UnionAction {
    /// Drop the whole subrecord (NULL not in the FK's allowed set, optional sub).
    /// SNDR.BNAM `base_descriptor` is required only in the AutoWeapon variant;
    /// dropping BNAM makes the descriptor non-AutoWeapon (a valid SNDR shape).
    StripSubrecord,
    /// Zero the FK in place at its union offset (NULL ∈ the FK's allowed set).
    /// PACK PTDA/PLDT location/target allow NULL (a package with no target/
    /// location idles), and the subrecord is structurally part of the package
    /// data-input block — nulling keeps the block intact, stripping would corrupt
    /// it (the paired-array lesson).
    NullFk,
}

/// Bytes-union wrong-type targets: `(record_sig, subrecord_sig, rule, action)`
/// whose FK lives in a raw `union`/`struct:`-codec blob that the decoded walk
/// can't reach (so `CLASS_C_TARGETS` / `KEYWORD_LIST_TARGETS` skip them — the
/// union field carries no `formlink_targets`). The FK sits at offset 0 (SNDR.BNAM
/// `base_descriptor` variant) or offset 4 (PACK PTDA/PLDT `[i32 type][FK]`); see
/// `validate_union_formid_target`.
///
/// SNDR.BNAM is a heterogeneous-size union: the 6-byte `values` variant is a
/// scalar (NOT a FK), the 4-byte `base_descriptor` variant is a SNDR FK. The
/// 4-byte length discriminates the FK variant, so a scalar `values` payload is
/// never touched.
const UNION_WRONGTYPE_TARGETS: &[(&str, &str, UnionTypeRule, UnionAction)] = &[
    (
        "SNDR",
        "BNAM",
        UnionTypeRule::AllowOnly(&["SNDR"]),
        UnionAction::StripSubrecord,
    ),
    (
        "PACK",
        "PTDA",
        UnionTypeRule::Deny(PACK_TARGET_DENY),
        UnionAction::NullFk,
    ),
    (
        "PACK",
        "PLDT",
        UnionTypeRule::Deny(PACK_TARGET_DENY),
        UnionAction::NullFk,
    ),
];

/// Record types that are NEVER a valid PACK package target (PTDA) or location
/// (PLDT) in ANY union variant: worldspace / structural / dialogue records. A
/// PTDA/PLDT FK resolving to one of these is a remap collision (a FO76 object-id
/// remapped onto an illegal FO4 package target), not a real target — the xEdit
/// "Found a LAND/CELL/DIAL/INFO/LVLI reference" cases.
const PACK_TARGET_DENY: &[&str] = &[
    "LAND", "CELL", "WRLD", "DIAL", "INFO", "NAVM", "NAVI", "LVLI",
];
const PACK_TARGET_SELF_TYPE: i32 = 6;

#[derive(Clone, Copy)]
enum ArrayRowAction {
    NullWrongType,
    DropNullOrWrongTypeRow,
}

/// `array_struct` row subrecords carrying a FK at a fixed per-row offset, which
/// the conversion crate keeps as raw `Bytes` (never decoded to a typed List) and
/// `struct_field_layout` doesn't expand (it only models `struct:` codecs). Each
/// tuple is `(record_sig, sub_sig, row_size, fk_offset, allow_only, action)`.
/// MGEF.SNDD is `array_struct:I,I` (8-byte rows `[type:u32, sound:SNDR]`);
/// xEdit rejects NULL/wrong-type sound rows, so they are removed. REGN.RDSA has
/// the regional ambient sound FK at offset 0 in each 12-byte row; its empty
/// sound row shape is valid, so wrong-type rows are nulled in place.
const ARRAY_ROW_FK_TARGETS: &[(&str, &str, usize, usize, &[&str], ArrayRowAction)] = &[
    (
        "MGEF",
        "SNDD",
        8,
        4,
        &["SNDR"],
        ArrayRowAction::DropNullOrWrongTypeRow,
    ),
    (
        "REGN",
        "RDSA",
        12,
        0,
        &["SNDR", "SOUN"],
        ArrayRowAction::NullWrongType,
    ),
];

const DOBJ_DNAM_ROW_LEN: usize = 8;
const DOBJ_FO76_ONLY_OBJECT_USE_TAGS: &[u32] = &[
    0x3346_4141, // AAF3
    0x484F_4641, // AFOH
    0x4441_4E4B, // KNAD
    0x5350_474C, // LGPS
];

const MGEF_ACTOR_VALUE_FIELD_IDS: &[&str] = &["actor_value", "actor_value_1"];
const FO4_HARDCODED_AVIF_LOCAL_IDS: &[u32] = &[
    0x0002D2, // AnimationMult
    0x0002D3, // WeapReloadSpeedMult
    0x0002D8, // ActionPointsRate
    0x0002DB, // RadsRate
    0x0002DE, // MeleeDamage
    0x000312, // weaponSpeedMult
    0x00032E, // PowerGenerated
    0x000331, // Food
    0x000332, // Water
    0x00034F, // Fatigue
    0x000359, // ActionPointsRateMult
    0x00035A, // ConditionRateMult
    0x00035C, // PowerArmorBattery
    0x00035F, // ReflectDamage
    0x00037F, // Sneak
];
const MAGIC_TARGET_SELF: u32 = 0;
const MAGIC_TARGET_AIMED: u32 = 2;
const NULL_REQUIRED_PARAM1_CONDITION_FUNCTIONS: &[u16] = &[
    14,  // GetActorValue: AVIF param
    67,  // GetInCell: CELL param
    149, // GetIsCurrentWeather: WTHR param
    248, // IsScenePlaying: SCEN param
];

// ---------------------------------------------------------------------------
// Master-aware encoded-formid → signature resolver
// ---------------------------------------------------------------------------

/// Resolves an encoded target FormID (`(load_index << 24) | object_id`) to its
/// record signature, consulting the OUTPUT plugin index first and the target
/// MASTERS on-demand. The output-only maps (`encoded_sigs`, `fk_to_sig`) miss
/// references to master records (e.g. an MGEF.SNDD sound pointing at a
/// Fallout4.esm REFR, or a KWDA keyword that resolves to a master STAT); the
/// master lookup is what lets the wrong-type strip fire for them.
struct MasterAwareSigResolver<'a> {
    /// Output-plugin encoded-formid → sig (built once, no per-record decode).
    output_encoded_sigs: &'a FxHashMap<u32, SigCode>,
    /// Target master plugin names, indexed by load order (load_index).
    target_masters: &'a [String],
    /// Handle IDs of the loaded target masters, parallel to `target_masters`.
    target_master_handle_ids: &'a [u64],
    /// Memoizes master-handle sig lookups (encoded id → resolved sig or None).
    master_cache: FxHashMap<u32, Option<SigCode>>,
}

impl<'a> MasterAwareSigResolver<'a> {
    fn new(
        output_encoded_sigs: &'a FxHashMap<u32, SigCode>,
        target_masters: &'a [String],
        target_master_handle_ids: &'a [u64],
    ) -> Self {
        Self {
            output_encoded_sigs,
            target_masters,
            target_master_handle_ids,
            master_cache: FxHashMap::default(),
        }
    }

    /// Resolve an encoded target FormID to its record sig, or `None` when it is
    /// null / resolves nowhere (output or master). A `None` result means the
    /// caller must treat the FK as dangling (NOT wrong-type) and leave it for the
    /// sweep / invalid-target fixups.
    fn sig_of_encoded(&mut self, session: &mut PluginSession, encoded: u32) -> Option<SigCode> {
        if encoded & 0x00FF_FFFF == 0 {
            return None; // null object-id
        }
        if let Some(sig) = self.output_encoded_sigs.get(&encoded) {
            return Some(*sig);
        }
        if let Some(cached) = self.master_cache.get(&encoded) {
            return *cached;
        }
        let resolved = self.lookup_master(session, encoded);
        self.master_cache.insert(encoded, resolved);
        resolved
    }

    fn lookup_master(&self, session: &mut PluginSession, encoded: u32) -> Option<SigCode> {
        let load_index = (encoded >> 24) as usize;
        let object_id = encoded & 0x00FF_FFFF;
        let master_name = self.target_masters.get(load_index)?;
        let handle_id = *self.target_master_handle_ids.get(load_index)?;
        let fk_str = format!("{master_name}:{object_id:06X}");
        let sig_str = session
            .record_signature_in_handle(handle_id, &fk_str)
            .ok()
            .flatten()?;
        SigCode::from_str(&sig_str).ok()
    }
}

fn encoded_sigs_from_fk_index_or_else<E>(
    fk_to_sig: &FxHashMap<(u32, Sym), SigCode>,
    interner: &StringInterner,
    target_masters: &[String],
    fallback: impl FnOnce() -> Result<FxHashMap<u32, SigCode>, E>,
) -> Result<FxHashMap<u32, SigCode>, E> {
    let mut encoded_sigs = FxHashMap::default();
    for (&(local, plugin), &sig) in fk_to_sig {
        let Some(encoded) =
            encode_target_form_id(FormKey { local, plugin }, interner, target_masters)
        else {
            continue;
        };
        if encoded_sigs
            .insert(encoded, sig)
            .is_some_and(|existing| existing != sig)
        {
            return fallback();
        }
    }
    Ok(encoded_sigs)
}

// ---------------------------------------------------------------------------
// Fixup
// ---------------------------------------------------------------------------

pub struct ValidateReferenceTargetTypesFixup;

impl Fixup for ValidateReferenceTargetTypesFixup {
    fn name(&self) -> &'static str {
        "validate_reference_target_types"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
        true
    }

    fn convergent(&self) -> bool {
        false
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();

        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        // The shared reference-target accessor lives on the compiled schema.
        let Some(compiled) = session
            .target_slot()
            .parsed
            .game
            .as_deref()
            .and_then(|game| compiled_schema_for_game(game).ok())
        else {
            return Ok(report);
        };

        // Only do work if at least one Class C record type (subrecord-level or
        // struct-internal) is present in the output plugin.
        let present_sigs = session
            .target_signatures()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let present = |record_sig: &str| present_sigs.iter().any(|s| s.as_str() == record_sig);
        let has_subrecord_class_c = CLASS_C_TARGETS.iter().any(|(r, _)| present(r));
        let has_struct_class_c = STRUCT_FK_SIGS.iter().any(|r| present(r));
        let has_keyword_class_c = KEYWORD_LIST_TARGETS.iter().any(|(r, _)| present(r));
        let has_union_class_c = UNION_WRONGTYPE_TARGETS.iter().any(|(r, ..)| present(r));
        let has_array_class_c = ARRAY_ROW_FK_TARGETS.iter().any(|(r, ..)| present(r));
        let has_magic_projectile_cleanup = present("SPEL");
        let has_dobj_object_use_cleanup = present("DOBJ");
        let has_null_required_param1_cleanup = ["ACTI", "SNDR", "TERM"]
            .iter()
            .any(|record_sig| present(record_sig));
        let has_orphan_alch_effect_cleanup = present("ALCH");
        if !has_subrecord_class_c
            && !has_struct_class_c
            && !has_keyword_class_c
            && !has_union_class_c
            && !has_array_class_c
            && !has_magic_projectile_cleanup
            && !has_dobj_object_use_cleanup
            && !has_null_required_param1_cleanup
            && !has_orphan_alch_effect_cleanup
            && !present("INFO")
        {
            return Ok(report);
        }

        let fk_to_sig = build_target_fk_sig_map(session, mapper.interner)?;
        if fk_to_sig.is_empty() {
            return Ok(report);
        }

        // Shared master-aware resolution inputs, built once: the output-plugin
        // encoded-formid → sig index plus the target master names/handles. The
        // subrecord-level, keyword-list, struct, and union passes all need the
        // master fallback so wrong-type refs that resolve to a MASTER record (not
        // just an output collision) are caught.
        let target_masters = session.target_masters().to_vec();
        let target_master_handle_ids = config.target_master_handle_ids.clone();
        let encoded_sigs = encoded_sigs_from_fk_index_or_else(
            &fk_to_sig,
            mapper.interner,
            &target_masters,
            || target_record_sigs_by_encoded_form_id(session, mapper.interner, &target_masters),
        )?;

        if has_dobj_object_use_cleanup {
            let dobj_sig = SigCode::from_str("DOBJ").expect("DOBJ sigcode");
            let fks = session
                .form_keys_of_sig(dobj_sig, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            let pre_filter = ["DNAM"];
            let mut changed_records = Vec::new();
            let mut total_dropped = 0u32;
            for fk in fks {
                if !session
                    .record_has_any_subrecord(&fk, &pre_filter)
                    .unwrap_or(false)
                {
                    continue;
                }
                let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                let dropped = drop_fo76_only_dobj_object_use_rows(&mut record, mapper.interner);
                if dropped > 0 {
                    changed_records.push(record);
                    total_dropped = total_dropped.saturating_add(dropped);
                }
            }
            let expected = changed_records.len();
            let replaced = session
                .replace_records_contents(changed_records, target_schema, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            if replaced != expected {
                return Err(FixupError::HandleError(format!(
                    "validate_reference_target_types DOBJ.DNAM cleanup replaced {replaced} of {expected} expected records"
                )));
            }
            report.records_changed = report
                .records_changed
                .saturating_add(replaced.try_into().unwrap_or(u32::MAX));
            report.records_dropped = report.records_dropped.saturating_add(total_dropped);
        }

        // CTDA rows whose function requires a non-null Parameter #1 reference
        // (AVIF/CELL/WTHR/SCEN in the observed buckets) cannot be repaired when
        // that parameter is zero. Drop the whole condition row, including any
        // trailing CIS1/CIS2 strings.
        if has_null_required_param1_cleanup {
            for record_sig in ["ACTI", "SNDR", "TERM"] {
                if !present_sigs.iter().any(|s| s.as_str() == record_sig) {
                    continue;
                }
                let Ok(sig) = SigCode::from_str(record_sig) else {
                    continue;
                };
                let fks = session
                    .form_keys_of_sig(sig, mapper.interner)
                    .map_err(|e| FixupError::HandleError(e.to_string()))?;
                let pre_filter = ["CTDA", "CTDT"];
                let mut changed_records = Vec::new();
                let mut total_dropped = 0u32;
                for fk in fks {
                    if !session
                        .record_has_any_subrecord(&fk, &pre_filter)
                        .unwrap_or(false)
                    {
                        continue;
                    }
                    let mut record =
                        match session.record_decoded(&fk, target_schema, mapper.interner) {
                            Ok(r) => r,
                            Err(_) => continue,
                        };
                    let dropped = drop_null_required_param1_conditions(&mut record);
                    if dropped > 0 {
                        changed_records.push(record);
                        total_dropped = total_dropped.saturating_add(dropped);
                    }
                }
                let expected = changed_records.len();
                let replaced = session
                    .replace_records_contents(changed_records, target_schema, mapper.interner)
                    .map_err(|e| FixupError::HandleError(e.to_string()))?;
                if replaced != expected {
                    return Err(FixupError::HandleError(format!(
                        "validate_reference_target_types {record_sig} CTDA cleanup replaced {replaced} of {expected} expected records"
                    )));
                }
                report.records_changed = report
                    .records_changed
                    .saturating_add(replaced.try_into().unwrap_or(u32::MAX));
                report.records_dropped = report.records_dropped.saturating_add(total_dropped);
            }
        }

        if has_orphan_alch_effect_cleanup {
            let alch_sig = SigCode::from_str("ALCH").expect("ALCH sigcode");
            let fks = session
                .form_keys_of_sig(alch_sig, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            let pre_filter = ["EFIT"];
            let mut changed_records = Vec::new();
            let mut total_dropped = 0u32;
            for fk in fks {
                if !session
                    .record_has_any_subrecord(&fk, &pre_filter)
                    .unwrap_or(false)
                {
                    continue;
                }
                let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                let dropped = drop_orphan_effect_data_fields(&mut record);
                if dropped > 0 {
                    changed_records.push(record);
                    total_dropped = total_dropped.saturating_add(dropped);
                }
            }
            let expected = changed_records.len();
            let replaced = session
                .replace_records_contents(changed_records, target_schema, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            if replaced != expected {
                return Err(FixupError::HandleError(format!(
                    "validate_reference_target_types ALCH orphan-effect cleanup replaced {replaced} of {expected} expected records"
                )));
            }
            report.records_changed = report
                .records_changed
                .saturating_add(replaced.try_into().unwrap_or(u32::MAX));
            report.records_dropped = report.records_dropped.saturating_add(total_dropped);
        }

        // INFO Speaker VTYP retarget/strip.
        // FO76 often uses a VTYP in INFO.ANAM. FO4's INFO.ANAM is an NPC_
        // speaker field. Prefer a deterministic NPC_ that uses that VTYP;
        // otherwise strip the optional ANAM rather than emitting an xEdit
        // wrong-type error.
        if present_sigs.iter().any(|s| s.as_str() == "INFO") {
            let info_sig = SigCode::from_str("INFO").expect("INFO sigcode");
            let anam_sig = SubrecordSig::from_str("ANAM").expect("ANAM subrecord");
            let mut voice_type_npcs =
                build_voice_type_npc_index(session, target_schema, mapper.interner)?;
            let output_plugin_name = session.target_slot().parsed.plugin_name.clone();
            let target_masters = session.target_masters().to_vec();
            let target_master_handle_ids = config.target_master_handle_ids.clone();
            let fks = session
                .form_keys_of_sig(info_sig, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;

            let mut missing_voice_types = FxHashSet::default();
            for fk in &fks {
                let record = match session.record_decoded(fk, target_schema, mapper.interner) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                let Some(speaker_fk) = record
                    .fields
                    .iter()
                    .find(|e| e.sig == anam_sig)
                    .and_then(|e| first_formkey(&e.value))
                else {
                    continue;
                };
                let speaker_sig = resolve_speaker_sig(
                    session,
                    mapper.interner,
                    &fk_to_sig,
                    &output_plugin_name,
                    &target_masters,
                    &target_master_handle_ids,
                    &speaker_fk,
                );
                if speaker_sig.is_some_and(|sig| sig.as_str() == "VTYP")
                    && !voice_type_npcs
                        .by_voice
                        .contains_key(&(speaker_fk.local, speaker_fk.plugin))
                {
                    missing_voice_types.insert(speaker_fk);
                }
            }

            if let Some(template) = voice_type_npcs.proxy_template.as_ref() {
                let mut missing_voice_types: Vec<_> = missing_voice_types.into_iter().collect();
                missing_voice_types.sort_by(|left, right| {
                    let left_plugin = mapper.interner.resolve(left.plugin).unwrap_or("");
                    let right_plugin = mapper.interner.resolve(right.plugin).unwrap_or("");
                    left_plugin
                        .cmp(right_plugin)
                        .then_with(|| left.local.cmp(&right.local))
                });
                let mut proxies = Vec::with_capacity(missing_voice_types.len());
                for voice_type in missing_voice_types {
                    let source_key = synthetic_info_speaker_source_key(voice_type, mapper.interner);
                    let proxy_fk = mapper.allocate_or_resolve(source_key, None, npc_sig());
                    proxies.push(build_info_speaker_proxy(
                        template,
                        proxy_fk,
                        voice_type,
                        mapper.interner,
                    )?);
                    voice_type_npcs
                        .by_voice
                        .insert((voice_type.local, voice_type.plugin), vec![proxy_fk]);
                }
                let added = session
                    .add_records(proxies, target_schema, mapper.interner)
                    .map_err(|e| FixupError::HandleError(e.to_string()))?;
                report.records_added = report
                    .records_added
                    .saturating_add(added.try_into().unwrap_or(u32::MAX));
            }

            let mut changed_records = Vec::new();
            let mut acted = 0u32;
            for fk in fks {
                let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                // Resolve the ANAM speaker FK's sig (output-first, masters
                // on-demand) BEFORE the in-place mutation, so the master VTYP
                // case (Class E) is visible. Needs `&mut session`, hence done
                // here rather than inside the retain_mut helper.
                let speaker_fk = record
                    .fields
                    .iter()
                    .find(|e| e.sig == anam_sig)
                    .and_then(|e| first_formkey(&e.value));
                let speaker_sig = match speaker_fk {
                    Some(sfk) => resolve_speaker_sig(
                        session,
                        mapper.interner,
                        &fk_to_sig,
                        &output_plugin_name,
                        &target_masters,
                        &target_master_handle_ids,
                        &sfk,
                    ),
                    None => None,
                };
                let outcome = retarget_or_strip_info_speaker(
                    &mut record,
                    anam_sig,
                    speaker_sig,
                    &voice_type_npcs.by_voice,
                );
                if outcome.changed {
                    acted = acted.saturating_add(outcome.acted);
                    changed_records.push(record);
                }
            }
            let expected = changed_records.len();
            let replaced = session
                .replace_records_contents(changed_records, target_schema, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            if replaced != expected {
                return Err(FixupError::HandleError(format!(
                    "validate_reference_target_types INFO speaker cleanup replaced {replaced} of {expected} expected records"
                )));
            }
            report.records_changed = report
                .records_changed
                .saturating_add(replaced.try_into().unwrap_or(u32::MAX));
            report.records_dropped = report.records_dropped.saturating_add(acted);
        }

        // ── Subrecord-level Class C (single-FK subrecords) ──────────────────
        struct ClassSubrecordWork {
            record_sig: &'static str,
            sub_sig: &'static str,
            sub: SubrecordSig,
            action: Action,
            strip_dangling: bool,
        }

        let mut class_work = Vec::new();
        for &(record_sig, sub_sig) in CLASS_C_TARGETS {
            if !present_sigs.iter().any(|s| s.as_str() == record_sig) {
                continue;
            }
            let Some(spec0) = compiled.allowed_targets(record_sig, sub_sig) else {
                continue;
            };
            let required = target_schema
                .subrecord_required(record_sig, sub_sig)
                .unwrap_or(false);
            let action = action_for(spec0.null_allowed, required);
            if action == Action::Leave {
                continue;
            }
            let Ok(sub) = SubrecordSig::from_str(sub_sig) else {
                continue;
            };
            let strip_dangling = action == Action::Strip
                && STRIP_DANGLING_SUBRECORDS
                    .iter()
                    .any(|(r, s)| *r == record_sig && *s == sub_sig);
            class_work.push(ClassSubrecordWork {
                record_sig,
                sub_sig,
                sub,
                action,
                strip_dangling,
            });
        }

        let mut processed_record_sigs = Vec::new();
        for head in &class_work {
            if processed_record_sigs.contains(&head.record_sig) {
                continue;
            }
            processed_record_sigs.push(head.record_sig);
            let group: Vec<&ClassSubrecordWork> = class_work
                .iter()
                .filter(|work| work.record_sig == head.record_sig)
                .collect();
            let Ok(sig) = SigCode::from_str(head.record_sig) else {
                continue;
            };
            let fks = session
                .form_keys_of_sig(sig, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;

            // Cheap pre-filter (REFR ~3.2M records): scan each record-sig once
            // for all relevant subrecords, then decode only records that carry
            // at least one target subrecord.
            let pre_filter: Vec<&str> = group.iter().map(|work| work.sub_sig).collect();
            let mut resolver = MasterAwareSigResolver::new(
                &encoded_sigs,
                &target_masters,
                &target_master_handle_ids,
            );
            let mut changed_records = Vec::new();
            let mut total_acted = 0u32;
            for fk in fks {
                if !session
                    .record_has_any_subrecord(&fk, &pre_filter)
                    .unwrap_or(false)
                {
                    continue;
                }
                let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                    Ok(r) => r,
                    Err(e) => {
                        let w = mapper
                            .interner
                            .intern(&format!("validate_reftype_read_err:{e}"));
                        report.warnings.push(w);
                        continue;
                    }
                };

                let mut changed = false;
                let mut acted = 0u32;
                for work in &group {
                    if !record.fields.iter().any(|entry| entry.sig == work.sub) {
                        continue;
                    }
                    let master_resolved = resolve_subrecord_fk_masters(
                        &record,
                        work.sub,
                        &fk_to_sig,
                        &mut resolver,
                        session,
                        mapper.interner,
                        &target_masters,
                    );
                    let allows = |sig: &str| -> bool {
                        compiled
                            .allowed_targets(work.record_sig, work.sub_sig)
                            .map(|spec| spec.allows_target(sig))
                            .unwrap_or(true)
                    };
                    let outcome = apply_to_record(
                        &mut record,
                        work.sub,
                        work.action,
                        work.strip_dangling,
                        &allows,
                        &fk_to_sig,
                        &master_resolved,
                    );
                    if outcome.changed {
                        changed = true;
                        acted = acted.saturating_add(outcome.acted);
                    }
                }
                if changed {
                    changed_records.push(record);
                    total_acted = total_acted.saturating_add(acted);
                }
            }
            let expected = changed_records.len();
            let replaced = session
                .replace_records_contents(changed_records, target_schema, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            if replaced != expected {
                return Err(FixupError::HandleError(format!(
                    "validate_reference_target_types {} subrecord cleanup replaced {replaced} of {expected} expected records",
                    head.record_sig
                )));
            }
            report.records_changed = report
                .records_changed
                .saturating_add(replaced.try_into().unwrap_or(u32::MAX));
            report.records_dropped = report.records_dropped.saturating_add(total_acted);
        }

        // ── Multi-entry list Class C (formid_array subrecords) ──────────────
        // Keyword ARRAYS (List of KYWD FormKeys): OMOD MNAM, FURN/WEAP/NPC_ KWDA.
        // A FO76 keyword's object-id, remapped to 07, can collide with an
        // unrelated output record (xEdit "KWDA -> Found a STAT/SCOL reference,
        // expected: KYWD,NULL"). The single-FK path above would strip the whole
        // subrecord (dropping valid keywords); instead filter the bad ENTRIES and
        // keep the rest. Dangling (non-null, unresolved) entries are left for the
        // sweep fixups, matching the conservative policy above.
        for &(record_sig, sub_sig) in KEYWORD_LIST_TARGETS {
            if !present_sigs.iter().any(|s| s.as_str() == record_sig)
                || compiled.allowed_targets(record_sig, sub_sig).is_none()
            {
                continue;
            }
            let allows = |sig: &str| -> bool {
                compiled
                    .allowed_targets(record_sig, sub_sig)
                    .map(|s| s.allows_target(sig))
                    .unwrap_or(true)
            };
            let (Ok(sig), Ok(sub)) = (
                SigCode::from_str(record_sig),
                SubrecordSig::from_str(sub_sig),
            ) else {
                continue;
            };
            let fks = session
                .form_keys_of_sig(sig, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            let pre_filter = [sub_sig];
            let mut resolver = MasterAwareSigResolver::new(
                &encoded_sigs,
                &target_masters,
                &target_master_handle_ids,
            );
            let mut changed_records = Vec::new();
            let mut total_acted = 0u32;
            for fk in fks {
                if !session
                    .record_has_any_subrecord(&fk, &pre_filter)
                    .unwrap_or(false)
                {
                    continue;
                }
                let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                // Master-aware: a KWDA entry's keyword id can resolve to a MASTER
                // record (e.g. a Fallout4.esm STAT/SNDR) the output-only index
                // misses. Pre-resolve those.
                let master_resolved = resolve_keyword_list_fk_masters(
                    &record,
                    sub,
                    &fk_to_sig,
                    &mut resolver,
                    session,
                    mapper.interner,
                    &target_masters,
                );
                let acted = filter_keyword_list_entries(
                    &mut record,
                    sub,
                    &allows,
                    &fk_to_sig,
                    &master_resolved,
                );
                if acted > 0 {
                    changed_records.push(record);
                    total_acted = total_acted.saturating_add(acted);
                }
            }
            let expected = changed_records.len();
            let replaced = session
                .replace_records_contents(changed_records, target_schema, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            if replaced != expected {
                return Err(FixupError::HandleError(format!(
                    "validate_reference_target_types {record_sig}.{sub_sig} keyword-list cleanup replaced {replaced} of {expected} expected records"
                )));
            }
            report.records_changed = report
                .records_changed
                .saturating_add(replaced.try_into().unwrap_or(u32::MAX));
            report.records_dropped = report.records_dropped.saturating_add(total_acted);
        }

        // ── Struct-internal Class C (FKs inside struct: codecs) ─────────────
        // RACE DATA, MGEF DATA, IDLE ANAM, COBJ FVPA components, LVLI LVLO,
        // MGEF SNDD sound, etc. These live in raw struct bytes the decoded walk
        // can't reach; validate each via the keystone struct_field_layout (shared
        // with the validator). MASTER-AWARE: MGEF.SNDD sounds point at FO4 master
        // SNDR/REFR records (e.g. `0010FBAB` in Fallout4.esm) the output-only
        // `encoded_sigs` map can't see; the resolver does the on-demand master
        // lookup so wrong-type ones are still caught.
        if has_struct_class_c {
            for record_sig in STRUCT_FK_SIGS {
                let sig = match SigCode::from_str(record_sig) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                if !present_sigs.iter().any(|s| s.as_str() == *record_sig) {
                    continue;
                }
                let fks = session
                    .form_keys_of_sig(sig, mapper.interner)
                    .map_err(|e| FixupError::HandleError(e.to_string()))?;
                let mut resolver = MasterAwareSigResolver::new(
                    &encoded_sigs,
                    &target_masters,
                    &target_master_handle_ids,
                );
                let mut changed_records = Vec::new();
                let mut total_nulled = 0u32;
                for fk in fks {
                    let mut record =
                        match session.record_decoded(&fk, target_schema, mapper.interner) {
                            Ok(r) => r,
                            Err(_) => continue,
                        };
                    // Resolve the record's struct FK target sigs (output-first,
                    // masters on-demand) BEFORE the mutation, since the resolver
                    // needs `&mut session` and the validator needs `&mut record`.
                    let resolved: FxHashMap<u32, SigCode> = collect_struct_fk_raws(
                        &record,
                        target_schema,
                        Some(crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION),
                    )
                    .into_iter()
                    .filter(|raw| !encoded_sigs.contains_key(raw))
                    .filter_map(|raw| resolver.sig_of_encoded(session, raw).map(|sig| (raw, sig)))
                    .collect();
                    let struct_fk = validate_struct_fk_fields(
                        &mut record,
                        target_schema,
                        Some(crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION),
                        &|raw| {
                            encoded_sigs
                                .get(&raw)
                                .or_else(|| resolved.get(&raw))
                                .copied()
                        },
                    );
                    let extra_nulled = if *record_sig == "MGEF" {
                        let actor_values_nulled = null_invalid_mgef_actor_values(
                            &mut record,
                            target_schema,
                            Some(
                                crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION,
                            ),
                            &encoded_sigs,
                            &resolved,
                            &target_masters,
                        );
                        let aimed_normalized = normalize_aimed_mgef_without_projectile(
                            &mut record,
                            target_schema,
                            Some(
                                crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION,
                            ),
                            &encoded_sigs,
                            &resolved,
                        );
                        actor_values_nulled.saturating_add(aimed_normalized)
                    } else {
                        0
                    };
                    if struct_fk.changed() || extra_nulled > 0 {
                        changed_records.push(record);
                        total_nulled = total_nulled
                            .saturating_add(struct_fk.nulled)
                            .saturating_add(extra_nulled);
                    }
                }
                let expected = changed_records.len();
                let replaced = session
                    .replace_records_contents(changed_records, target_schema, mapper.interner)
                    .map_err(|e| FixupError::HandleError(e.to_string()))?;
                if replaced != expected {
                    return Err(FixupError::HandleError(format!(
                        "validate_reference_target_types {record_sig} struct cleanup replaced {replaced} of {expected} expected records"
                    )));
                }
                report.records_changed = report
                    .records_changed
                    .saturating_add(replaced.try_into().unwrap_or(u32::MAX));
                report.records_dropped = report.records_dropped.saturating_add(total_nulled);
            }
        }

        // ── FO4 magic projectile invariant ─────────────────────────────────
        // FO4 CK rejects AIMED magic items unless at least one base effect has a
        // valid projectile. Converted FO76 aura/concentration records can carry
        // Aimed delivery with no projectile, so normalize only records whose
        // output MGEFs are known and none has a projectile-backed effect.
        if present_sigs.iter().any(|s| s.as_str() == "SPEL") {
            let mgef_projectiles = if present_sigs.iter().any(|s| s.as_str() == "MGEF") {
                build_mgef_projectile_index(
                    session,
                    target_schema,
                    mapper.interner,
                    &encoded_sigs,
                    &target_masters,
                    &target_master_handle_ids,
                )?
            } else {
                FxHashMap::default()
            };
            let spel_sig = SigCode::from_str("SPEL").expect("SPEL sigcode");
            let fks = session
                .form_keys_of_sig(spel_sig, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            let pre_filter = ["SPIT"];
            let mut changed_records = Vec::new();
            let mut total_acted = 0u32;
            for fk in fks {
                if !session
                    .record_has_any_subrecord(&fk, &pre_filter)
                    .unwrap_or(false)
                {
                    continue;
                }
                let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                let acted = normalize_aimed_spell_without_projectile_effects(
                    &mut record,
                    target_schema,
                    Some(crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION),
                    &mgef_projectiles,
                );
                if acted > 0 {
                    changed_records.push(record);
                    total_acted = total_acted.saturating_add(acted);
                }
            }
            let expected = changed_records.len();
            let replaced = session
                .replace_records_contents(changed_records, target_schema, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            if replaced != expected {
                return Err(FixupError::HandleError(format!(
                    "validate_reference_target_types SPEL aimed-projectile cleanup replaced {replaced} of {expected} expected records"
                )));
            }
            report.records_changed = report
                .records_changed
                .saturating_add(replaced.try_into().unwrap_or(u32::MAX));
            report.records_dropped = report.records_dropped.saturating_add(total_acted);
        }

        // ── Bytes-union wrong-type (FKs inside union codec blobs) ───────────
        // SNDR.BNAM (base_descriptor variant), PACK PTDA/PLDT (type-selected
        // location/target). The FK is invisible to the decoded walk (the union
        // field carries no formlink_targets), so the subrecord-level and
        // keyword-list passes above skip it. MASTER-AWARE for the same reason as
        // the struct path.
        let has_union_wrongtype = UNION_WRONGTYPE_TARGETS
            .iter()
            .any(|(record_sig, _, _, _)| present_sigs.iter().any(|s| s.as_str() == *record_sig));
        if has_union_wrongtype {
            for &(record_sig, sub_sig, rule, action) in UNION_WRONGTYPE_TARGETS {
                let (Ok(sig), Ok(sub)) = (
                    SigCode::from_str(record_sig),
                    SubrecordSig::from_str(sub_sig),
                ) else {
                    continue;
                };
                if !present_sigs.iter().any(|s| s.as_str() == record_sig) {
                    continue;
                }
                let fks = session
                    .form_keys_of_sig(sig, mapper.interner)
                    .map_err(|e| FixupError::HandleError(e.to_string()))?;
                let pre_filter = [sub_sig];
                let mut resolver = MasterAwareSigResolver::new(
                    &encoded_sigs,
                    &target_masters,
                    &target_master_handle_ids,
                );
                let mut changed_records = Vec::new();
                let mut total_acted = 0u32;
                for fk in fks {
                    if !session
                        .record_has_any_subrecord(&fk, &pre_filter)
                        .unwrap_or(false)
                    {
                        continue;
                    }
                    let mut record =
                        match session.record_decoded(&fk, target_schema, mapper.interner) {
                            Ok(r) => r,
                            Err(_) => continue,
                        };
                    // Resolve each union-FK's target sig before mutation.
                    let raws = collect_union_fk_raws(&record, sub_sig, sub);
                    let resolved: FxHashMap<u32, SigCode> = raws
                        .into_iter()
                        .filter_map(|raw| resolver.sig_of_encoded(session, raw).map(|s| (raw, s)))
                        .collect();
                    let mut acted = validate_union_formid_target(
                        &mut record,
                        sub_sig,
                        sub,
                        rule,
                        action,
                        &resolved,
                    );
                    if record_sig == "PACK" && sub_sig == "PTDA" {
                        let nonpersistent_refs = collect_nonpersistent_pack_ptda_refs(
                            &record,
                            session,
                            target_schema,
                            mapper.interner,
                            &target_masters,
                            &target_master_handle_ids,
                            &resolved,
                        );
                        acted = acted.saturating_add(benignify_pack_ptda_refs(
                            &mut record,
                            &nonpersistent_refs,
                        ));
                    }
                    if acted > 0 {
                        changed_records.push(record);
                        total_acted = total_acted.saturating_add(acted);
                    }
                }
                let expected = changed_records.len();
                let replaced = session
                    .replace_records_contents(changed_records, target_schema, mapper.interner)
                    .map_err(|e| FixupError::HandleError(e.to_string()))?;
                if replaced != expected {
                    return Err(FixupError::HandleError(format!(
                        "validate_reference_target_types {record_sig}.{sub_sig} union cleanup replaced {replaced} of {expected} expected records"
                    )));
                }
                report.records_changed = report
                    .records_changed
                    .saturating_add(replaced.try_into().unwrap_or(u32::MAX));
                report.records_dropped = report.records_dropped.saturating_add(total_acted);
            }
        }

        // ── SCOL ONAM+DATA paired-part strip + FO76-only MNAM strip ────────
        // FO76 allows nested SCOL-in-SCOL (ONAM codec='struct:I,I' with SCOL in
        // formlink_targets). FO4 does not (ONAM codec='formid', SCOL absent from
        // formlink_targets). An ONAM whose target resolves to SCOL must be dropped
        // TOGETHER with its immediately-following DATA (required, paired by
        // scope_id='parts') — stripping ONAM alone orphans a DATA, which causes
        // xEdit "out of order" errors (the paired-array lockstep rule).
        // Late LOD synthesis can also append FO76 SCOL.MNAM chunks after the
        // translation-map drop list has run; FO4 xEdit rejects those on SCOL.
        if present_sigs.iter().any(|s| s.as_str() == "SCOL") {
            let scol_sig = SigCode::from_str("SCOL").expect("SCOL sigcode");
            let fks = session
                .form_keys_of_sig(scol_sig, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            let pre_filter = ["ONAM", "MNAM"];
            let mut resolver = MasterAwareSigResolver::new(
                &encoded_sigs,
                &target_masters,
                &target_master_handle_ids,
            );
            let mut changed_records = Vec::new();
            let mut total_acted = 0u32;
            for fk in fks {
                if !session
                    .record_has_any_subrecord(&fk, &pre_filter)
                    .unwrap_or(false)
                {
                    continue;
                }
                let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                let master_resolved = resolve_scol_onam_fk_masters(
                    &record,
                    &fk_to_sig,
                    &mut resolver,
                    session,
                    mapper.interner,
                    &target_masters,
                );
                let acted = strip_scol_wrong_type_onam_data_pairs(
                    &mut record,
                    &fk_to_sig,
                    &master_resolved,
                )
                .saturating_add(strip_scol_mnam_subrecords(&mut record));
                if acted > 0 {
                    changed_records.push(record);
                    total_acted = total_acted.saturating_add(acted);
                }
            }
            let expected = changed_records.len();
            let replaced = session
                .replace_records_contents(changed_records, target_schema, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            if replaced != expected {
                return Err(FixupError::HandleError(format!(
                    "validate_reference_target_types SCOL ONAM cleanup replaced {replaced} of {expected} expected records"
                )));
            }
            report.records_changed = report
                .records_changed
                .saturating_add(replaced.try_into().unwrap_or(u32::MAX));
            report.records_dropped = report.records_dropped.saturating_add(total_acted);
        }

        // ── array_struct row FK (raw Bytes the struct_field_layout misses) ──
        // MGEF.SNDD (`array_struct:I,I`): per-row sound FK at offset 4. MASTER-
        // AWARE so a sound resolving to a FO4-master REFR (xEdit "Found a REFR,
        // expected SNDR") is nulled.
        let has_array_row_fk = ARRAY_ROW_FK_TARGETS
            .iter()
            .any(|(record_sig, ..)| present_sigs.iter().any(|s| s.as_str() == *record_sig));
        if has_array_row_fk {
            for &(record_sig, sub_sig, row_size, fk_offset, allow_only, action) in
                ARRAY_ROW_FK_TARGETS
            {
                let (Ok(sig), Ok(sub)) = (
                    SigCode::from_str(record_sig),
                    SubrecordSig::from_str(sub_sig),
                ) else {
                    continue;
                };
                if !present_sigs.iter().any(|s| s.as_str() == record_sig) {
                    continue;
                }
                let fks = session
                    .form_keys_of_sig(sig, mapper.interner)
                    .map_err(|e| FixupError::HandleError(e.to_string()))?;
                let pre_filter = [sub_sig];
                let mut resolver = MasterAwareSigResolver::new(
                    &encoded_sigs,
                    &target_masters,
                    &target_master_handle_ids,
                );
                let mut changed_records = Vec::new();
                let mut total_acted = 0u32;
                for fk in fks {
                    if !session
                        .record_has_any_subrecord(&fk, &pre_filter)
                        .unwrap_or(false)
                    {
                        continue;
                    }
                    let mut record =
                        match session.record_decoded(&fk, target_schema, mapper.interner) {
                            Ok(r) => r,
                            Err(_) => continue,
                        };
                    let raws = collect_array_row_fk_raws(&record, sub, row_size, fk_offset);
                    let resolved: FxHashMap<u32, SigCode> = raws
                        .into_iter()
                        .filter_map(|raw| resolver.sig_of_encoded(session, raw).map(|s| (raw, s)))
                        .collect();
                    let acted = match action {
                        ArrayRowAction::NullWrongType => null_array_row_wrong_type_fks(
                            &mut record,
                            sub,
                            row_size,
                            fk_offset,
                            allow_only,
                            &resolved,
                        ),
                        ArrayRowAction::DropNullOrWrongTypeRow => {
                            drop_array_rows_with_null_or_wrong_type_fks(
                                &mut record,
                                sub,
                                row_size,
                                fk_offset,
                                allow_only,
                                &resolved,
                            )
                        }
                    };
                    if acted > 0 {
                        changed_records.push(record);
                        total_acted = total_acted.saturating_add(acted);
                    }
                }
                let expected = changed_records.len();
                let replaced = session
                    .replace_records_contents(changed_records, target_schema, mapper.interner)
                    .map_err(|e| FixupError::HandleError(e.to_string()))?;
                if replaced != expected {
                    return Err(FixupError::HandleError(format!(
                        "validate_reference_target_types {record_sig}.{sub_sig} array-row cleanup replaced {replaced} of {expected} expected records"
                    )));
                }
                report.records_changed = report
                    .records_changed
                    .saturating_add(replaced.try_into().unwrap_or(u32::MAX));
                report.records_dropped = report.records_dropped.saturating_add(total_acted);
            }
        }

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Post-copy REFR placed-child subrecord strip
// ---------------------------------------------------------------------------

/// REFR-only `(record_sig, subrecord_sig)` Class C entries (subset of
/// `CLASS_C_TARGETS`) carried by placed children. These live on REFR records,
/// which are in `skip_records` and re-inserted by the FO76→FO4 phase-6 cell-slice
/// copy that runs AFTER the registered fixup phase. So the in-phase
/// `ValidateReferenceTargetTypesFixup` run sees ZERO of them, leaving their
/// wrong-type / no-FO4-home FKs (xEdit "Found a REFR/INFO/LVLI/LAND reference,
/// expected RFGP/LAYR/REFR") in the output. This post-copy entrypoint re-runs the
/// identical subrecord-level Class C strip against the now-complete plugin.
const REFR_PLACED_CHILD_CLASS_C: &[(&str, &str)] =
    &[("REFR", "XRFG"), ("REFR", "XLYR"), ("REFR", "XASP")];

/// Strip wrong-type / no-FO4-home REFR placed-child subrecords (XRFG→RFGP,
/// XLYR→LAYR, XASP→REFR) against the now-complete output plugin. Mirrors the
/// subrecord-level Class C loop in `run_with_session`, restricted to the REFR
/// placed-child slots, and reuses the same `apply_to_record` / master-aware
/// resolver. Called from `ConversionRun::repair_placed_child_refs` (the post-copy
/// hook), AFTER phase-6 re-inserts the placed children — the registered in-phase
/// run cannot see these records (REFR ∈ `skip_records`).
pub fn strip_refr_placed_child_subrecords(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
) -> Result<FixupReport, FixupError> {
    let mut report = FixupReport::empty();

    let target_schema = config
        .target_schema
        .as_deref()
        .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

    let Some(compiled) = session
        .target_slot()
        .parsed
        .game
        .as_deref()
        .and_then(|game| compiled_schema_for_game(game).ok())
    else {
        return Ok(report);
    };

    // No REFR in the output (e.g. a non-worldspace port) → nothing to do.
    let present_sigs = session
        .target_signatures()
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    if !present_sigs.iter().any(|s| s.as_str() == "REFR") {
        return Ok(report);
    }

    let fk_to_sig = build_target_fk_sig_map(session, mapper.interner)?;
    let target_masters = session.target_masters().to_vec();
    let target_master_handle_ids = config.target_master_handle_ids.clone();
    let encoded_sigs =
        encoded_sigs_from_fk_index_or_else(&fk_to_sig, mapper.interner, &target_masters, || {
            target_record_sigs_by_encoded_form_id(session, mapper.interner, &target_masters)
        })?;

    struct RefrPlacedChildRule {
        sub_sig: &'static str,
        sub: SubrecordSig,
        action: Action,
        strip_dangling: bool,
    }

    let mut rules = Vec::new();
    for &(record_sig, sub_sig) in REFR_PLACED_CHILD_CLASS_C {
        if record_sig != "REFR" {
            continue;
        }
        let Some(spec0) = compiled.allowed_targets(record_sig, sub_sig) else {
            continue;
        };
        let required = target_schema
            .subrecord_required(record_sig, sub_sig)
            .unwrap_or(false);
        let action = action_for(spec0.null_allowed, required);
        if action == Action::Leave {
            continue;
        }
        let Ok(sub) = SubrecordSig::from_str(sub_sig) else {
            continue;
        };
        let strip_dangling = action == Action::Strip
            && STRIP_DANGLING_SUBRECORDS
                .iter()
                .any(|(r, s)| *r == record_sig && *s == sub_sig);
        rules.push(RefrPlacedChildRule {
            sub_sig,
            sub,
            action,
            strip_dangling,
        });
    }

    if rules.is_empty() {
        return Ok(report);
    }

    let sig = SigCode::from_str("REFR").expect("REFR sigcode");
    let fks = session
        .form_keys_of_sig(sig, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let pre_filter: Vec<&str> = rules.iter().map(|rule| rule.sub_sig).collect();
    let mut resolver =
        MasterAwareSigResolver::new(&encoded_sigs, &target_masters, &target_master_handle_ids);
    let mut changed_records = Vec::new();
    let mut records_dropped = 0u32;
    for fk in fks {
        if !session
            .record_has_any_subrecord(&fk, &pre_filter)
            .unwrap_or(false)
        {
            continue;
        }
        let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let mut changed = false;
        for rule in &rules {
            let master_resolved = resolve_subrecord_fk_masters(
                &record,
                rule.sub,
                &fk_to_sig,
                &mut resolver,
                session,
                mapper.interner,
                &target_masters,
            );
            let sub_sig = rule.sub_sig;
            let allows = |sig: &str| -> bool {
                compiled
                    .allowed_targets("REFR", sub_sig)
                    .map(|spec| spec.allows_target(sig))
                    .unwrap_or(true)
            };
            let outcome = apply_to_record(
                &mut record,
                rule.sub,
                rule.action,
                rule.strip_dangling,
                &allows,
                &fk_to_sig,
                &master_resolved,
            );
            if outcome.changed {
                changed = true;
                records_dropped = records_dropped.saturating_add(outcome.acted);
            }
        }
        if changed {
            changed_records.push(record);
        }
    }

    let expected = changed_records.len();
    if expected > 0 {
        let replaced = session
            .replace_records_contents(changed_records, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "strip_refr_placed_child_subrecords replaced {replaced} of {expected} expected records"
            )));
        }
        report.records_changed = replaced.try_into().unwrap_or(u32::MAX);
        report.records_dropped = records_dropped;
    }

    Ok(report)
}

// ---------------------------------------------------------------------------
// Record-level mutation (extracted for unit-test access)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Outcome {
    changed: bool,
    /// Number of FK slots nulled or subrecords stripped.
    acted: u32,
}

/// Apply `action` to every `sub`-signature subrecord of `record` whose FK
/// resolves to a record signature for which `allows` is false.
///
/// Resolution is OUTPUT-first (`fk_to_sig`) then a per-record MASTER fallback
/// (`master_resolved`, keyed identically): the output-only index misses refs to
/// master records (e.g. an XASP REFR whose target lives in Fallout4.esm). `allows` is
/// the shared schema accessor (`RefTargetSpec::allows_target`), passed in so this
/// fixup never re-implements the allow-check. A FK that is null, or that resolves
/// nowhere (output or master), is left untouched (dangling refs are owned by the
/// invalid-target / sweep fixups).
fn apply_to_record(
    record: &mut Record,
    sub: SubrecordSig,
    action: Action,
    strip_dangling: bool,
    allows: &dyn Fn(&str) -> bool,
    fk_to_sig: &FxHashMap<(u32, Sym), SigCode>,
    master_resolved: &FxHashMap<(u32, Sym), SigCode>,
) -> Outcome {
    let mut outcome = Outcome::default();
    let rec_sig = record.sig;
    let rec_local = record.form_key.local;
    let sub_str = sub.as_str();

    record.fields.retain_mut(|entry| {
        if entry.sig != sub {
            return true;
        }
        let Some(fk) = first_formkey(&entry.value) else {
            // A present-but-null `formid` subrecord decodes to `FieldValue::None`
            // (source_read: raw==0 => None), so `first_formkey` finds no FK. This
            // is STILL a "Found NULL, expected X" error in xEdit. For a Strip
            // action (optional, NULL-disallowed) drop the whole subrecord so it's
            // absent (valid) rather than present-but-NULL — this is the bulk
            // DIAL BNAM / FACT VENC null source. Non-None values with no FK leaf
            // (shouldn't happen for these single-FK subrecords) are left as-is.
            if action == Action::Strip && matches!(entry.value, FieldValue::None) {
                outcome.changed = true;
                outcome.acted += 1;
                crate::drop_trace::trace(
                    "typecheck.strip",
                    rec_sig.as_str(),
                    rec_local,
                    sub_str,
                    "present-but-null subrecord (None), Strip action",
                );
                return false;
            }
            return true;
        };

        // A NULL FK in a Strip-action (optional, NULL-disallowed) subrecord is
        // itself a "Found NULL, expected X" error — strip the whole subrecord so
        // it's absent (valid) rather than present-but-NULL. This also cleans up
        // the dangling refs that fix_invalid_target_formkeys nulled upstream
        // (the bulk DIAL BNAM / FACT VENC C_null source).
        if fk.local == 0 {
            if action == Action::Strip {
                outcome.changed = true;
                outcome.acted += 1;
                crate::drop_trace::trace(
                    "typecheck.strip",
                    rec_sig.as_str(),
                    rec_local,
                    sub_str,
                    "null FK (local==0), Strip action",
                );
                return false;
            }
            return true; // Null/Leave: a null FK is already as-good-as-it-gets
        }

        let resolved_sig = fk_to_sig
            .get(&(fk.local, fk.plugin))
            .or_else(|| master_resolved.get(&(fk.local, fk.plugin)));
        let Some(resolved_sig) = resolved_sig else {
            // Dangling (non-null, unresolved in output AND masters). Normally not
            // ours (sweep fixups own it). But for a subrecord whose only valid FO4
            // target type is never carried (RFGP/LAYR), a dangling FK can never
            // become valid — strip it under a Strip action.
            if strip_dangling {
                outcome.changed = true;
                outcome.acted += 1;
                crate::drop_trace::trace(
                    "typecheck.strip",
                    rec_sig.as_str(),
                    rec_local,
                    sub_str,
                    "dangling FK (unresolved in output+masters), strip_dangling",
                );
                return false;
            }
            return true;
        };
        if allows(resolved_sig.as_str()) {
            return true; // legal type
        }

        match action {
            Action::Null => {
                null_formkeys(&mut entry.value);
                outcome.changed = true;
                outcome.acted += 1;
                crate::drop_trace::trace(
                    "typecheck.null",
                    rec_sig.as_str(),
                    rec_local,
                    sub_str,
                    "FK resolves to wrong target type, Null action",
                );
                true
            }
            Action::Strip => {
                outcome.changed = true;
                outcome.acted += 1;
                crate::drop_trace::trace(
                    "typecheck.strip",
                    rec_sig.as_str(),
                    rec_local,
                    sub_str,
                    "FK resolves to wrong target type, Strip action",
                );
                false // drop the subrecord
            }
            Action::Leave => true, // required NULL-disallowed: only retarget fixes
        }
    });

    outcome
}

/// Filter wrong-type entries out of a `formid_array` (List) subrecord, keeping
/// the valid ones. Returns the number of entries removed.
///
/// An entry is REMOVED only when its FK resolves (OUTPUT `fk_to_sig` first, then
/// the per-record MASTER fallback `master_resolved`) to a record signature for
/// which `allows` is false — a positively-known wrong type (e.g. OMOD MNAM
/// keyword pointing at a SCOL, FURN KWDA at a master STAT). Null FKs and dangling
/// (non-null, unresolved in output AND masters) FKs are KEPT — dangling refs are
/// owned by the sweep / invalid-target fixups, matching `apply_to_record`'s
/// conservative policy. If every entry is removed the now-empty subrecord is
/// dropped entirely.
fn filter_keyword_list_entries(
    record: &mut Record,
    sub: SubrecordSig,
    allows: &dyn Fn(&str) -> bool,
    fk_to_sig: &FxHashMap<(u32, Sym), SigCode>,
    master_resolved: &FxHashMap<(u32, Sym), SigCode>,
) -> u32 {
    let mut removed = 0u32;
    let trace_kw = crate::drop_trace::enabled();
    let rec_sig = record.sig.as_str().to_string();
    let rec_local = record.form_key.local;
    let sub_str = std::str::from_utf8(&sub.0).unwrap_or("?").to_string();
    record.fields.retain_mut(|entry| {
        if entry.sig != sub {
            return true;
        }
        let FieldValue::List(items) = &mut entry.value else {
            return true;
        };
        items.retain(|item| {
            let Some(fk) = first_formkey(item) else {
                return true; // not a FK leaf — keep
            };
            if fk.local == 0 {
                return true; // null — keep
            }
            let resolved = fk_to_sig
                .get(&(fk.local, fk.plugin))
                .or_else(|| master_resolved.get(&(fk.local, fk.plugin)));
            let watched = trace_kw && crate::drop_trace::is_dlc_kw_watched(fk.local);
            let Some(resolved) = resolved else {
                if watched {
                    crate::drop_trace::trace(
                        "validate_keyword_list",
                        &rec_sig,
                        rec_local,
                        &sub_str,
                        &format!("keep-dangling {:06X}", fk.local),
                    );
                }
                return true; // dangling — owned by the sweep fixups
            };
            if allows(resolved.as_str()) {
                if watched {
                    crate::drop_trace::trace(
                        "validate_keyword_list",
                        &rec_sig,
                        rec_local,
                        &sub_str,
                        &format!("keep-legal {:06X} ({})", fk.local, resolved.as_str()),
                    );
                }
                return true; // legal type
            }
            if watched {
                crate::drop_trace::trace(
                    "validate_keyword_list",
                    &rec_sig,
                    rec_local,
                    &sub_str,
                    &format!(
                        "DROP wrong-type {:06X} (resolved {})",
                        fk.local,
                        resolved.as_str()
                    ),
                );
            }
            removed += 1;
            false // positively wrong type — drop this entry
        });
        // Drop the whole subrecord if filtering emptied it (an empty keyword
        // array is not meaningful and would re-encode to a zero-length sub).
        !items.is_empty()
    });
    removed
}

/// MASTER fallback for keyword-LIST entries: for each entry FK of the `sub`
/// list subrecord that misses the output `fk_to_sig` index, resolve its sig via
/// the masters (cached). Same key space as `fk_to_sig` so the filter consults it
/// transparently.
#[allow(clippy::too_many_arguments)]
fn resolve_keyword_list_fk_masters(
    record: &Record,
    sub: SubrecordSig,
    fk_to_sig: &FxHashMap<(u32, Sym), SigCode>,
    resolver: &mut MasterAwareSigResolver,
    session: &mut PluginSession,
    interner: &StringInterner,
    target_masters: &[String],
) -> FxHashMap<(u32, Sym), SigCode> {
    let mut out = FxHashMap::default();
    for entry in &record.fields {
        if entry.sig != sub {
            continue;
        }
        let FieldValue::List(items) = &entry.value else {
            continue;
        };
        for item in items {
            let Some(fk) = first_formkey(item) else {
                continue;
            };
            if fk.local == 0 || fk_to_sig.contains_key(&(fk.local, fk.plugin)) {
                continue;
            }
            if out.contains_key(&(fk.local, fk.plugin)) {
                continue;
            }
            let Some(encoded) = encode_target_fk(&fk, interner, target_masters) else {
                continue;
            };
            if let Some(sig) = resolver.sig_of_encoded(session, encoded) {
                out.insert((fk.local, fk.plugin), sig);
            }
        }
    }
    out
}

struct VoiceTypeNpcIndex {
    by_voice: FxHashMap<(u32, Sym), Vec<FormKey>>,
    proxy_template: Option<Record>,
}

fn npc_sig() -> SigCode {
    SigCode::from_str("NPC_").expect("NPC_ sigcode")
}

fn build_voice_type_npc_index(
    session: &mut PluginSession,
    target_schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<VoiceTypeNpcIndex, FixupError> {
    let npc_sig = npc_sig();
    let vtck_sig = SubrecordSig::from_str("VTCK").expect("VTCK subrecord");
    let mut voice_type_npcs: FxHashMap<(u32, Sym), Vec<FormKey>> = FxHashMap::default();
    let mut proxy_template: Option<Record> = None;
    let fks = session
        .form_keys_of_sig(npc_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    for fk in fks {
        let record = match session.record_decoded(&fk, target_schema, interner) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if record.flags.contains(RecordFlags::DELETED) {
            continue;
        }
        for entry in &record.fields {
            if entry.sig != vtck_sig {
                continue;
            }
            let Some(voice_type) = first_formkey(&entry.value) else {
                continue;
            };
            if voice_type.local != 0 {
                voice_type_npcs
                    .entry((voice_type.local, voice_type.plugin))
                    .or_default()
                    .push(record.form_key);
            }
            if proxy_template
                .as_ref()
                .is_none_or(|current| prefer_info_speaker_template(&record, current, interner))
            {
                proxy_template = Some(record.clone());
            }
        }
    }
    Ok(VoiceTypeNpcIndex {
        by_voice: voice_type_npcs,
        proxy_template,
    })
}

fn prefer_info_speaker_template(
    candidate: &Record,
    current: &Record,
    interner: &StringInterner,
) -> bool {
    let tplt_sig = SubrecordSig::from_str("TPLT").expect("TPLT subrecord");
    let score = |record: &Record| {
        (
            record.fields.iter().any(|entry| entry.sig == tplt_sig),
            record.fields.len(),
            interner.resolve(record.form_key.plugin).unwrap_or(""),
            record.form_key.local,
        )
    };
    score(candidate) < score(current)
}

fn synthetic_info_speaker_source_key(voice_type: FormKey, interner: &StringInterner) -> FormKey {
    let plugin = interner.resolve(voice_type.plugin).unwrap_or("unknown");
    FormKey {
        local: voice_type.local,
        plugin: interner.intern(&format!("__synth_info_speaker__{plugin}")),
    }
}

fn build_info_speaker_proxy(
    template: &Record,
    form_key: FormKey,
    voice_type: FormKey,
    interner: &StringInterner,
) -> Result<Record, FixupError> {
    let edid_sig = SubrecordSig::from_str("EDID").map_err(FixupError::SchemaError)?;
    let vmad_sig = SubrecordSig::from_str("VMAD").map_err(FixupError::SchemaError)?;
    let vtck_sig = SubrecordSig::from_str("VTCK").map_err(FixupError::SchemaError)?;
    let plugin = interner.resolve(voice_type.plugin).unwrap_or("unknown");
    let plugin_token: String = plugin
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect();
    let eid = interner.intern(&format!(
        "BACUP_InfoSpeaker_{plugin_token}_{:06X}",
        voice_type.local
    ));

    let mut proxy = template.clone();
    proxy.form_key = form_key;
    proxy.eid = Some(eid);
    proxy.flags = RecordFlags::empty();
    proxy.warnings.clear();
    proxy.fields.retain(|entry| entry.sig != vmad_sig);

    if let Some(entry) = proxy.fields.iter_mut().find(|entry| entry.sig == edid_sig) {
        entry.value = FieldValue::String(eid);
    } else {
        proxy.fields.insert(
            0,
            FieldEntry {
                sig: edid_sig,
                value: FieldValue::String(eid),
            },
        );
    }
    let Some(entry) = proxy.fields.iter_mut().find(|entry| entry.sig == vtck_sig) else {
        return Err(FixupError::Other(
            "INFO speaker proxy template has no VTCK".to_string(),
        ));
    };
    entry.value = FieldValue::FormKey(voice_type);
    Ok(proxy)
}

/// Resolve an INFO ANAM speaker FK's target signature, consulting the output
/// plugin index FIRST, then the target MASTERS on-demand. FO76 INFO speakers
/// commonly point at a *master* VTYP (e.g. Fallout4.esm `VTYP:0000002D`), which
/// the output-only `fk_to_sig` map can't see — the master lookup is what lets
/// the wrong-type retarget/strip fire for those (Class E). Bounded: called once
/// per INFO that has a non-null ANAM whose FK misses the output map.
fn resolve_speaker_sig(
    session: &mut PluginSession,
    interner: &StringInterner,
    fk_to_sig: &FxHashMap<(u32, Sym), SigCode>,
    output_plugin_name: &str,
    target_masters: &[String],
    target_master_handle_ids: &[u64],
    fk: &FormKey,
) -> Option<SigCode> {
    if fk.local == 0 {
        return None;
    }
    if let Some(sig) = fk_to_sig.get(&(fk.local, fk.plugin)) {
        return Some(*sig);
    }
    let plugin = interner.resolve(fk.plugin)?;
    // Own-plugin FK not in the index → genuinely dangling, not a master record.
    if plugin.eq_ignore_ascii_case(output_plugin_name) {
        return None;
    }
    for (master_name, handle_id) in target_masters.iter().zip(target_master_handle_ids.iter()) {
        if !plugin.eq_ignore_ascii_case(master_name) {
            continue;
        }
        let fk_str = format!("{master_name}:{:06X}", fk.local & 0x00FF_FFFF);
        let sig_str = session
            .record_signature_in_handle(*handle_id, &fk_str)
            .ok()
            .flatten()?;
        return SigCode::from_str(&sig_str).ok();
    }
    None
}

fn retarget_or_strip_info_speaker(
    record: &mut Record,
    sub: SubrecordSig,
    speaker_sig: Option<SigCode>,
    voice_type_npcs: &FxHashMap<(u32, Sym), Vec<FormKey>>,
) -> Outcome {
    let mut outcome = Outcome::default();
    record.fields.retain_mut(|entry| {
        if entry.sig != sub {
            return true;
        }
        let Some(speaker_fk) = first_formkey(&entry.value) else {
            if matches!(entry.value, FieldValue::None) {
                outcome.changed = true;
                outcome.acted += 1;
                return false;
            }
            return true;
        };
        if speaker_fk.local == 0 {
            outcome.changed = true;
            outcome.acted += 1;
            return false;
        }
        let Some(resolved_sig) = speaker_sig else {
            return true; // unresolved (output + masters) — sweep fixups' domain
        };
        match resolved_sig.as_str() {
            "NPC_" => true,
            "VTYP" => match voice_type_npcs
                .get(&(speaker_fk.local, speaker_fk.plugin))
                .and_then(|candidates| candidates.iter().min_by_key(|candidate| candidate.local))
            {
                Some(candidate) => {
                    entry.value = FieldValue::FormKey(*candidate);
                    outcome.changed = true;
                    outcome.acted += 1;
                    true
                }
                _ => {
                    outcome.changed = true;
                    outcome.acted += 1;
                    false
                }
            },
            _ => {
                outcome.changed = true;
                outcome.acted += 1;
                false
            }
        }
    });
    outcome
}

/// First `FormKey` leaf in a field value (depth-first), if any.
fn first_formkey(value: &FieldValue) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(fk) => Some(*fk),
        FieldValue::List(items) => items.iter().find_map(first_formkey),
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, v)| first_formkey(v)),
        _ => None,
    }
}

/// Zero every `FormKey` leaf in `value` in place.
fn null_formkeys(value: &mut FieldValue) {
    match value {
        FieldValue::FormKey(fk) => fk.local = 0,
        FieldValue::List(items) => items.iter_mut().for_each(null_formkeys),
        FieldValue::Struct(fields) => fields.iter_mut().for_each(|(_, v)| null_formkeys(v)),
        _ => {}
    }
}

fn read_u32_le(bytes: &[u8], off: usize) -> Option<u32> {
    let slot = bytes.get(off..off + 4)?;
    Some(u32::from_le_bytes([slot[0], slot[1], slot[2], slot[3]]))
}

fn write_u32_le(bytes: &mut [u8], off: usize, value: u32) -> bool {
    let Some(slot) = bytes.get_mut(off..off + 4) else {
        return false;
    };
    slot.copy_from_slice(&value.to_le_bytes());
    true
}

fn struct_field_offset(
    schema: &AuthoringSchema,
    record_sig: &str,
    sub_sig: &str,
    form_version: Option<u16>,
    field_id: &str,
) -> Option<usize> {
    schema
        .struct_field_layout_versioned(record_sig, sub_sig, form_version)
        .into_iter()
        .find(|field| field.field_id == field_id && field.width == 4)
        .map(|field| field.offset)
}

fn struct_field_offsets(
    schema: &AuthoringSchema,
    record_sig: &str,
    sub_sig: &str,
    form_version: Option<u16>,
    field_ids: &[&str],
) -> Vec<usize> {
    schema
        .struct_field_layout_versioned(record_sig, sub_sig, form_version)
        .into_iter()
        .filter(|field| field.width == 4 && field_ids.contains(&field.field_id))
        .map(|field| field.offset)
        .collect()
}

fn raw_resolves_to_sig(
    raw: u32,
    sig_name: &str,
    encoded_sigs: &FxHashMap<u32, SigCode>,
    resolved: &FxHashMap<u32, SigCode>,
) -> bool {
    encoded_sigs
        .get(&raw)
        .or_else(|| resolved.get(&raw))
        .is_some_and(|sig| sig.as_str() == sig_name)
}

fn is_fo4_hardcoded_actor_value_raw(raw: u32, target_masters: &[String]) -> bool {
    let load_index = (raw >> 24) as usize;
    let local = raw & 0x00FF_FFFF;
    target_masters
        .get(load_index)
        .is_some_and(|master| master.eq_ignore_ascii_case("Fallout4.esm"))
        && FO4_HARDCODED_AVIF_LOCAL_IDS.contains(&local)
}

fn is_valid_mgef_actor_value_raw(
    raw: u32,
    encoded_sigs: &FxHashMap<u32, SigCode>,
    resolved: &FxHashMap<u32, SigCode>,
    target_masters: &[String],
) -> bool {
    raw == 0
        || raw_resolves_to_sig(raw, "AVIF", encoded_sigs, resolved)
        || is_fo4_hardcoded_actor_value_raw(raw, target_masters)
}

fn null_invalid_mgef_actor_values(
    record: &mut Record,
    schema: &AuthoringSchema,
    form_version: Option<u16>,
    encoded_sigs: &FxHashMap<u32, SigCode>,
    resolved: &FxHashMap<u32, SigCode>,
    target_masters: &[String],
) -> u32 {
    let Ok(data_sub) = SubrecordSig::from_str("DATA") else {
        return 0;
    };
    let offsets = struct_field_offsets(
        schema,
        "MGEF",
        "DATA",
        form_version,
        MGEF_ACTOR_VALUE_FIELD_IDS,
    );
    if offsets.is_empty() {
        return 0;
    }

    let mut nulled = 0u32;
    for entry in record.fields.iter_mut() {
        if entry.sig != data_sub {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        for &off in &offsets {
            let Some(raw) = read_u32_le(bytes, off) else {
                continue;
            };
            if is_valid_mgef_actor_value_raw(raw, encoded_sigs, resolved, target_masters) {
                continue;
            }
            if write_u32_le(bytes, off, 0) {
                nulled = nulled.saturating_add(1);
            }
        }
    }
    nulled
}

fn mgef_projectile_raw(
    record: &Record,
    schema: &AuthoringSchema,
    form_version: Option<u16>,
) -> Option<u32> {
    let data_sub = SubrecordSig::from_str("DATA").ok()?;
    let projectile_off = struct_field_offset(schema, "MGEF", "DATA", form_version, "projectile")?;
    record.fields.iter().find_map(|entry| {
        if entry.sig != data_sub {
            return None;
        }
        let FieldValue::Bytes(bytes) = &entry.value else {
            return None;
        };
        read_u32_le(bytes, projectile_off)
    })
}

fn mgef_projectile_is_valid(
    raw: u32,
    encoded_sigs: &FxHashMap<u32, SigCode>,
    resolved: &FxHashMap<u32, SigCode>,
) -> bool {
    raw != 0 && raw_resolves_to_sig(raw, "PROJ", encoded_sigs, resolved)
}

fn normalize_aimed_mgef_without_projectile(
    record: &mut Record,
    schema: &AuthoringSchema,
    form_version: Option<u16>,
    encoded_sigs: &FxHashMap<u32, SigCode>,
    resolved: &FxHashMap<u32, SigCode>,
) -> u32 {
    let Ok(data_sub) = SubrecordSig::from_str("DATA") else {
        return 0;
    };
    let Some(delivery_off) = struct_field_offset(schema, "MGEF", "DATA", form_version, "delivery")
    else {
        return 0;
    };
    let Some(projectile_off) =
        struct_field_offset(schema, "MGEF", "DATA", form_version, "projectile")
    else {
        return 0;
    };

    let mut acted = 0u32;
    for entry in record.fields.iter_mut() {
        if entry.sig != data_sub {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        if read_u32_le(bytes, delivery_off) != Some(MAGIC_TARGET_AIMED) {
            continue;
        }
        let projectile = read_u32_le(bytes, projectile_off).unwrap_or(0);
        if mgef_projectile_is_valid(projectile, encoded_sigs, resolved) {
            continue;
        }
        if write_u32_le(bytes, delivery_off, MAGIC_TARGET_SELF) {
            acted = acted.saturating_add(1);
        }
    }
    acted
}

fn build_mgef_projectile_index(
    session: &mut PluginSession,
    schema: &AuthoringSchema,
    interner: &StringInterner,
    encoded_sigs: &FxHashMap<u32, SigCode>,
    target_masters: &[String],
    target_master_handle_ids: &[u64],
) -> Result<FxHashMap<(u32, Sym), bool>, FixupError> {
    let mut out = FxHashMap::default();
    let mgef_sig = SigCode::from_str("MGEF").expect("MGEF sigcode");
    let fks = session
        .form_keys_of_sig(mgef_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let mut resolver =
        MasterAwareSigResolver::new(encoded_sigs, target_masters, target_master_handle_ids);
    for fk in fks {
        let record = match session.record_decoded(&fk, schema, interner) {
            Ok(record) => record,
            Err(_) => continue,
        };
        let projectile = mgef_projectile_raw(
            &record,
            schema,
            Some(crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION),
        )
        .unwrap_or(0);
        let mut resolved = FxHashMap::default();
        if projectile != 0 && !encoded_sigs.contains_key(&projectile) {
            if let Some(sig) = resolver.sig_of_encoded(session, projectile) {
                resolved.insert(projectile, sig);
            }
        }
        out.insert(
            (record.form_key.local, record.form_key.plugin),
            mgef_projectile_is_valid(projectile, encoded_sigs, &resolved),
        );
    }
    Ok(out)
}

fn normalize_aimed_spell_without_projectile_effects(
    record: &mut Record,
    schema: &AuthoringSchema,
    form_version: Option<u16>,
    mgef_projectiles: &FxHashMap<(u32, Sym), bool>,
) -> u32 {
    let (Ok(spit_sub), Ok(efid_sub)) = (
        SubrecordSig::from_str("SPIT"),
        SubrecordSig::from_str("EFID"),
    ) else {
        return 0;
    };
    let Some(target_type_off) =
        struct_field_offset(schema, "SPEL", "SPIT", form_version, "target_type")
    else {
        return 0;
    };

    let effects: Vec<FormKey> = record
        .fields
        .iter()
        .filter(|entry| entry.sig == efid_sub)
        .filter_map(|entry| first_formkey(&entry.value))
        .filter(|fk| fk.local != 0)
        .collect();
    if !effects.is_empty()
        && effects
            .iter()
            .any(|fk| !mgef_projectiles.contains_key(&(fk.local, fk.plugin)))
    {
        return 0;
    }
    if effects
        .iter()
        .any(|fk| mgef_projectiles.get(&(fk.local, fk.plugin)).copied() == Some(true))
    {
        return 0;
    }

    let mut acted = 0u32;
    for entry in record.fields.iter_mut() {
        if entry.sig != spit_sub {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        if read_u32_le(bytes, target_type_off) != Some(MAGIC_TARGET_AIMED) {
            continue;
        }
        if write_u32_le(bytes, target_type_off, MAGIC_TARGET_SELF) {
            acted = acted.saturating_add(1);
        }
    }
    acted
}

// ---------------------------------------------------------------------------
// Struct / union FK raw collection (for master-aware pre-resolution)
// ---------------------------------------------------------------------------

/// Collect every struct-internal FK raw (encoded u32) of `record`, mirroring
/// `validate_struct_fk_fields`'s walk so the master pre-resolution and the
/// validation agree on which slots exist and at what offset. Returns the raw
/// (encoded) FormID at each width-4 `formlink_targets` field offset.
fn collect_struct_fk_raws(
    record: &Record,
    schema: &AuthoringSchema,
    form_version: Option<u16>,
) -> Vec<u32> {
    let record_sig = record.sig.as_str().to_string();
    let mut raws = Vec::new();
    for entry in &record.fields {
        let FieldValue::Bytes(bytes) = &entry.value else {
            continue;
        };
        let sub_sig = entry.sig.as_str();
        let layout = schema.struct_field_layout_versioned(&record_sig, sub_sig, form_version);
        for field in &layout {
            if field.formlink_targets.is_empty() || field.width != 4 {
                continue;
            }
            let off = field.offset;
            let Some(slot) = bytes.get(off..off + 4) else {
                continue;
            };
            let raw = u32::from_le_bytes([slot[0], slot[1], slot[2], slot[3]]);
            if raw & 0x00FF_FFFF != 0 {
                raws.push(raw);
            }
        }
    }
    raws
}

/// Whether a `sub_sig` bytes-union subrecord blob carries its FK at offset 0
/// (SNDR.BNAM `base_descriptor`, a bare 4-byte formid) or at offset 4 (PACK
/// PTDA/PLDT, `[i32 type][FK @4]`). Returns `None` when the blob's active variant
/// is NOT a FormID variant (SNDR.BNAM `values` 6-byte scalar; a PTDA/PLDT type
/// selector that designates a scalar variant) so its bytes are never read as a FK.
fn union_fk_offset(sub_sig: &str, bytes: &[u8]) -> Option<usize> {
    match sub_sig {
        // SNDR.BNAM: heterogeneous-size union. The 4-byte `base_descriptor`
        // variant is a SNDR formid; the 6-byte `values` variant is a scalar (the
        // benign floor). Length discriminates, so a scalar is never touched.
        "BNAM" => (bytes.len() == 4).then_some(0),
        // PACK PTDA/PLDT: `[i32 type][union value @4]`. The type selector decides
        // whether offset 4 is a FormID (shared with the remap gate so detect and
        // remap agree).
        "PTDA" | "PLDT" => {
            if bytes.len() < 8 {
                return None;
            }
            let kind = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            union_type_holds_formid(sub_sig, kind).then_some(4)
        }
        _ => None,
    }
}

/// Collect the FK raws of every `sub`-signature bytes-union subrecord of
/// `record`, for master-aware pre-resolution.
fn collect_union_fk_raws(record: &Record, sub_sig: &str, sub: SubrecordSig) -> Vec<u32> {
    let mut raws = Vec::new();
    for entry in &record.fields {
        if entry.sig != sub {
            continue;
        }
        let FieldValue::Bytes(bytes) = &entry.value else {
            continue;
        };
        let Some(off) = union_fk_offset(sub_sig, bytes) else {
            continue;
        };
        let Some(slot) = bytes.get(off..off + 4) else {
            continue;
        };
        let raw = u32::from_le_bytes([slot[0], slot[1], slot[2], slot[3]]);
        if raw & 0x00FF_FFFF != 0 {
            raws.push(raw);
        }
    }
    raws
}

/// Act on a bytes-union subrecord whose FK resolves to a positively-wrong target
/// type (e.g. SNDR.BNAM `base_descriptor` → CELL/LAND, PACK PTDA/PLDT → LAND/
/// CELL/DIAL/INFO). Returns the number of subrecords acted on.
///
/// Acts when the FK resolves (via `resolved`, output + master) to a sig the
/// `rule` judges wrong-type. A dangling FK is normally left for the sweep fixups;
/// the exception is a Strip-subrecord union such as SNDR.BNAM base_descriptor,
/// whose optional FK subrecord is invalid when unresolved or null. The `action`
/// is per-subrecord: SNDR.BNAM drops the whole subrecord; PACK PTDA/PLDT zero
/// the FK in place, with PTDA rewritten to the benign Self selector.
fn validate_union_formid_target(
    record: &mut Record,
    sub_sig: &str,
    sub: SubrecordSig,
    rule: UnionTypeRule,
    action: UnionAction,
    resolved: &FxHashMap<u32, SigCode>,
) -> u32 {
    let mut acted = 0u32;
    record.fields.retain_mut(|entry| {
        if entry.sig != sub {
            return true;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            return true;
        };
        let Some(off) = union_fk_offset(sub_sig, bytes) else {
            return true; // scalar variant — not a FK
        };
        let Some(slot) = bytes.get(off..off + 4) else {
            return true;
        };
        let raw = u32::from_le_bytes([slot[0], slot[1], slot[2], slot[3]]);
        let Some(sig) = resolved.get(&raw) else {
            if action == UnionAction::StripSubrecord {
                acted += 1;
                return false;
            }
            return true; // null or dangling — sweep fixups' domain
        };
        if !rule.is_wrong_type(sig.as_str()) {
            return true; // legal type
        }
        acted += 1;
        match action {
            UnionAction::StripSubrecord => false, // drop the whole subrecord
            UnionAction::NullFk => {
                bytes[off..off + 4].copy_from_slice(&0u32.to_le_bytes());
                if sub_sig == "PTDA" {
                    bytes[0..4].copy_from_slice(&PACK_TARGET_SELF_TYPE.to_le_bytes());
                }
                true // keep the subrecord with a NULL (allowed) target
            }
        }
    });
    acted
}

fn raw_condition_function_id(bytes: &[u8]) -> Option<u16> {
    (bytes.len() >= 10).then(|| u16::from_le_bytes([bytes[8], bytes[9]]))
}

fn raw_condition_parameter_1(bytes: &[u8]) -> Option<u32> {
    (bytes.len() >= 16).then(|| u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]))
}

fn drop_null_required_param1_conditions(record: &mut Record) -> u32 {
    let mut dropped = 0u32;
    let mut dropping_condition_strings = false;
    record.fields.retain(|entry| match &entry.sig.0 {
        b"CTDA" | b"CTDT" => {
            let should_drop = match &entry.value {
                FieldValue::Bytes(bytes) => {
                    let function_id = raw_condition_function_id(bytes).unwrap_or(0);
                    let parameter_1 = raw_condition_parameter_1(bytes).unwrap_or(0);
                    parameter_1 == 0
                        && NULL_REQUIRED_PARAM1_CONDITION_FUNCTIONS.contains(&function_id)
                }
                _ => false,
            };
            dropping_condition_strings = should_drop;
            if should_drop {
                dropped = dropped.saturating_add(1);
            }
            !should_drop
        }
        b"CIS1" | b"CIS2" => !dropping_condition_strings,
        _ => {
            dropping_condition_strings = false;
            true
        }
    });
    if dropped > 0 {
        record.sync_condition_count();
    }
    dropped
}

fn drop_orphan_effect_data_fields(record: &mut Record) -> u32 {
    let (Ok(efid_sub), Ok(efit_sub)) = (
        SubrecordSig::from_str("EFID"),
        SubrecordSig::from_str("EFIT"),
    ) else {
        return 0;
    };

    let mut dropped = 0u32;
    let mut pending_efid = false;
    let mut retained: smallvec::SmallVec<[crate::record::FieldEntry; 8]> =
        smallvec::SmallVec::new();
    for entry in record.fields.drain(..) {
        if entry.sig == efid_sub {
            pending_efid = true;
            retained.push(entry);
            continue;
        }
        if entry.sig == efit_sub {
            if pending_efid {
                pending_efid = false;
                retained.push(entry);
            } else {
                dropped = dropped.saturating_add(1);
            }
            continue;
        }
        pending_efid = false;
        retained.push(entry);
    }
    record.fields = retained;
    dropped
}

fn collect_nonpersistent_pack_ptda_refs(
    record: &Record,
    session: &mut PluginSession,
    target_schema: &AuthoringSchema,
    interner: &StringInterner,
    target_masters: &[String],
    target_master_handle_ids: &[u64],
    resolved: &FxHashMap<u32, SigCode>,
) -> FxHashSet<u32> {
    let Ok(ptda_sub) = SubrecordSig::from_str("PTDA") else {
        return FxHashSet::default();
    };
    let output_master_index = target_masters.len();
    let output_plugin = session.target_slot().parsed.plugin_name.clone();
    let mut invalid = FxHashSet::default();

    for entry in &record.fields {
        if entry.sig != ptda_sub {
            continue;
        }
        let FieldValue::Bytes(bytes) = &entry.value else {
            continue;
        };
        let Some(off) = union_fk_offset("PTDA", bytes) else {
            continue;
        };
        let Some(raw) = read_u32_le(bytes, off) else {
            continue;
        };
        if !resolved.get(&raw).is_some_and(|sig| sig.as_str() == "REFR") {
            continue;
        }
        let object_id = raw & 0x00FF_FFFF;
        if object_id == 0 {
            continue;
        }
        let load_index = (raw >> 24) as usize;
        let decoded = if load_index == output_master_index {
            let fk = FormKey {
                local: object_id,
                plugin: interner.intern(&output_plugin),
            };
            session.record_decoded(&fk, target_schema, interner).ok()
        } else {
            let Some(master_name) = target_masters.get(load_index) else {
                continue;
            };
            let Some(handle_id) = target_master_handle_ids.get(load_index) else {
                continue;
            };
            let fk = FormKey {
                local: object_id,
                plugin: interner.intern(master_name),
            };
            session
                .record_decoded_in_handle(*handle_id, &fk, target_schema, interner)
                .ok()
        };
        if decoded.is_some_and(|target| !target.flags.contains(RecordFlags::PERSISTENT)) {
            invalid.insert(raw);
        }
    }

    invalid
}

fn benignify_pack_ptda_refs(record: &mut Record, invalid_raws: &FxHashSet<u32>) -> u32 {
    if invalid_raws.is_empty() {
        return 0;
    }
    let Ok(ptda_sub) = SubrecordSig::from_str("PTDA") else {
        return 0;
    };
    let mut acted = 0u32;
    for entry in record.fields.iter_mut() {
        if entry.sig != ptda_sub {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        let Some(off) = union_fk_offset("PTDA", bytes) else {
            continue;
        };
        let Some(raw) = read_u32_le(bytes, off) else {
            continue;
        };
        if !invalid_raws.contains(&raw) {
            continue;
        }
        bytes[0..4].copy_from_slice(&PACK_TARGET_SELF_TYPE.to_le_bytes());
        bytes[off..off + 4].copy_from_slice(&0u32.to_le_bytes());
        acted = acted.saturating_add(1);
    }
    acted
}

/// For the single-FK subrecord `sub` of `record`, resolve its FK's target sig
/// via the MASTERS when it misses the output `fk_to_sig` index, returning a map
/// keyed `(local, plugin_sym)` (same key space as `fk_to_sig`) so `apply_to_record`
/// can consult it as a fallback. Only non-null FKs absent from the output index
/// trigger a (cached) master lookup; resolved master records are inserted, output
/// hits and pure danglers are not.
#[allow(clippy::too_many_arguments)]
fn resolve_subrecord_fk_masters(
    record: &Record,
    sub: SubrecordSig,
    fk_to_sig: &FxHashMap<(u32, Sym), SigCode>,
    resolver: &mut MasterAwareSigResolver,
    session: &mut PluginSession,
    interner: &StringInterner,
    target_masters: &[String],
) -> FxHashMap<(u32, Sym), SigCode> {
    let mut out = FxHashMap::default();
    for entry in &record.fields {
        if entry.sig != sub {
            continue;
        }
        let Some(fk) = first_formkey(&entry.value) else {
            continue;
        };
        if fk.local == 0 || fk_to_sig.contains_key(&(fk.local, fk.plugin)) {
            continue; // null or already resolvable in the output index
        }
        let Some(encoded) = encode_target_fk(&fk, interner, target_masters) else {
            continue;
        };
        if let Some(sig) = resolver.sig_of_encoded(session, encoded) {
            out.insert((fk.local, fk.plugin), sig);
        }
    }
    out
}

/// Collect the per-row FK raws (encoded u32) of every `sub`-signature
/// `array_struct` Bytes subrecord of `record`, reading offset `fk_offset` of each
/// `row_size`-byte row. Skips null FKs.
fn collect_array_row_fk_raws(
    record: &Record,
    sub: SubrecordSig,
    row_size: usize,
    fk_offset: usize,
) -> Vec<u32> {
    let mut raws = Vec::new();
    for entry in &record.fields {
        if entry.sig != sub {
            continue;
        }
        let FieldValue::Bytes(bytes) = &entry.value else {
            continue;
        };
        if row_size == 0 || bytes.len() % row_size != 0 {
            continue; // not a clean row layout — don't misread
        }
        for row in 0..(bytes.len() / row_size) {
            let off = row * row_size + fk_offset;
            let Some(slot) = bytes.get(off..off + 4) else {
                continue;
            };
            let raw = u32::from_le_bytes([slot[0], slot[1], slot[2], slot[3]]);
            if raw & 0x00FF_FFFF != 0 {
                raws.push(raw);
            }
        }
    }
    raws
}

/// Null (in place) every per-row FK of an `array_struct` Bytes subrecord whose
/// resolved target sig is outside `allow_only`. Returns the number of rows nulled.
/// A FK that doesn't resolve (dangling) is left for the sweep fixups.
fn null_array_row_wrong_type_fks(
    record: &mut Record,
    sub: SubrecordSig,
    row_size: usize,
    fk_offset: usize,
    allow_only: &[&str],
    resolved: &FxHashMap<u32, SigCode>,
) -> u32 {
    let mut nulled = 0u32;
    for entry in record.fields.iter_mut() {
        if entry.sig != sub {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        if row_size == 0 || bytes.len() % row_size != 0 {
            continue;
        }
        for row in 0..(bytes.len() / row_size) {
            let off = row * row_size + fk_offset;
            let Some(slot) = bytes.get(off..off + 4) else {
                continue;
            };
            let raw = u32::from_le_bytes([slot[0], slot[1], slot[2], slot[3]]);
            let Some(sig) = resolved.get(&raw) else {
                continue; // null or dangling — sweep fixups' domain
            };
            if allow_only.contains(&sig.as_str()) {
                continue; // legal type
            }
            bytes[off..off + 4].copy_from_slice(&0u32.to_le_bytes());
            nulled += 1;
        }
    }
    nulled
}

fn drop_array_rows_with_null_or_wrong_type_fks(
    record: &mut Record,
    sub: SubrecordSig,
    row_size: usize,
    fk_offset: usize,
    allow_only: &[&str],
    resolved: &FxHashMap<u32, SigCode>,
) -> u32 {
    let mut dropped = 0u32;
    let mut new_fields: smallvec::SmallVec<[crate::record::FieldEntry; 8]> =
        smallvec::SmallVec::new();

    for mut entry in record.fields.drain(..) {
        if entry.sig != sub {
            new_fields.push(entry);
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            new_fields.push(entry);
            continue;
        };
        if row_size == 0 || bytes.len() % row_size != 0 {
            new_fields.push(entry);
            continue;
        }

        let mut kept = Vec::with_capacity(bytes.len());
        for row in bytes.chunks_exact(row_size) {
            let off = fk_offset;
            let Some(slot) = row.get(off..off + 4) else {
                kept.extend_from_slice(row);
                continue;
            };
            let raw = u32::from_le_bytes([slot[0], slot[1], slot[2], slot[3]]);
            let should_drop = raw & 0x00FF_FFFF == 0
                || resolved
                    .get(&raw)
                    .is_some_and(|sig| !allow_only.contains(&sig.as_str()));
            if should_drop {
                dropped = dropped.saturating_add(1);
            } else {
                kept.extend_from_slice(row);
            }
        }

        if kept.is_empty() {
            continue;
        }
        *bytes = smallvec::SmallVec::from_vec(kept);
        new_fields.push(entry);
    }

    record.fields = new_fields;
    dropped
}

fn drop_fo76_only_dobj_object_use_rows(record: &mut Record, interner: &StringInterner) -> u32 {
    let Ok(dnam_sub) = SubrecordSig::from_str("DNAM") else {
        return 0;
    };
    let mut dropped = 0u32;
    let mut new_fields: smallvec::SmallVec<[crate::record::FieldEntry; 8]> =
        smallvec::SmallVec::new();

    for mut entry in record.fields.drain(..) {
        if entry.sig != dnam_sub {
            new_fields.push(entry);
            continue;
        }
        match &mut entry.value {
            FieldValue::Bytes(bytes) if bytes.len() % DOBJ_DNAM_ROW_LEN == 0 => {
                let mut kept = Vec::with_capacity(bytes.len());
                for row in bytes.chunks_exact(DOBJ_DNAM_ROW_LEN) {
                    let Some(tag) = read_u32_le(row, 0) else {
                        kept.extend_from_slice(row);
                        continue;
                    };
                    if DOBJ_FO76_ONLY_OBJECT_USE_TAGS.contains(&tag) {
                        dropped = dropped.saturating_add(1);
                    } else {
                        kept.extend_from_slice(row);
                    }
                }
                if kept.is_empty() {
                    continue;
                }
                *bytes = smallvec::SmallVec::from_vec(kept);
                new_fields.push(entry);
            }
            FieldValue::List(items) => {
                let before = items.len();
                items.retain(|item| {
                    dobj_object_use_tag(item, interner)
                        .is_none_or(|tag| !DOBJ_FO76_ONLY_OBJECT_USE_TAGS.contains(&tag))
                });
                dropped = dropped.saturating_add((before - items.len()) as u32);
                if !items.is_empty() {
                    new_fields.push(entry);
                }
            }
            _ => new_fields.push(entry),
        }
    }

    record.fields = new_fields;
    dropped
}

fn dobj_object_use_tag(value: &FieldValue, interner: &StringInterner) -> Option<u32> {
    match value {
        FieldValue::Struct(fields) => fields
            .iter()
            .find_map(|(name, value)| {
                let name = interner.resolve(*name)?;
                let canonical = canonical_field_name(name);
                (canonical == "tag"
                    || canonical == "objectuse"
                    || canonical == "objectsuse"
                    || canonical == "objectsobjectuse")
                    .then_some(value)
            })
            .and_then(field_value_u32),
        _ => field_value_u32(value),
    }
}

fn canonical_field_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn field_value_u32(value: &FieldValue) -> Option<u32> {
    match value {
        FieldValue::Uint(n) => u32::try_from(*n).ok(),
        FieldValue::Int(n) if *n >= 0 => u32::try_from(*n as u64).ok(),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => read_u32_le(bytes, 0),
        _ => None,
    }
}

/// Encode a target `FormKey` to its on-disk `(load_index << 24) | object_id`
/// form, matching `rewrite_raw_object_template_formids::encode_target_form_id`.
/// `load_index == target_masters.len()` denotes the output plugin itself.
fn encode_target_fk(
    fk: &FormKey,
    interner: &StringInterner,
    target_masters: &[String],
) -> Option<u32> {
    if fk.local == 0 {
        return Some(0);
    }
    let plugin_name = interner.resolve(fk.plugin)?;
    // Only refs to a plugin that is actually in `target_masters` can be
    // type-resolved. Callers exclude output-plugin records upstream (`fk_to_sig`),
    // so a plugin missing here is an unresolvable master ref — return None so it is
    // kept as dangling. It must NOT fall into the `target_masters.len()` sentinel
    // bucket, which `output_encoded_sigs` reuses for the output's own records: that
    // collision makes an out-of-list master ref (e.g. an OMOD MNAM keyword pointing
    // at DLCNukaWorld.esm:033B61) resolve to an unrelated output record sharing the
    // object id, and it then gets wrongly dropped as a wrong-type entry.
    let load_index = target_masters
        .iter()
        .position(|master| master.eq_ignore_ascii_case(plugin_name))?;
    if load_index > u8::MAX as usize || fk.local > 0x00FF_FFFF {
        return None;
    }
    Some(((load_index as u32) << 24) | fk.local)
}

// ---------------------------------------------------------------------------
// SCOL ONAM+DATA paired-part strip
// ---------------------------------------------------------------------------

/// Pre-resolve every SCOL ONAM FormKey that misses the output `fk_to_sig` index
/// against the target masters, returning a `(local, plugin_sym) → SigCode` map
/// (same key space as `fk_to_sig`) for use as a fallback in
/// `strip_scol_wrong_type_onam_data_pairs`.
fn resolve_scol_onam_fk_masters(
    record: &Record,
    fk_to_sig: &FxHashMap<(u32, Sym), SigCode>,
    resolver: &mut MasterAwareSigResolver,
    session: &mut PluginSession,
    interner: &StringInterner,
    target_masters: &[String],
) -> FxHashMap<(u32, Sym), SigCode> {
    let Ok(onam_sub) = SubrecordSig::from_str("ONAM") else {
        return FxHashMap::default();
    };
    let mut out = FxHashMap::default();
    for entry in &record.fields {
        if entry.sig != onam_sub {
            continue;
        }
        let Some(fk) = first_formkey(&entry.value) else {
            continue;
        };
        if fk.local == 0 || fk_to_sig.contains_key(&(fk.local, fk.plugin)) {
            continue; // null or already resolvable in the output index
        }
        let Some(encoded) = encode_target_fk(&fk, interner, target_masters) else {
            continue;
        };
        if let Some(sig) = resolver.sig_of_encoded(session, encoded) {
            out.insert((fk.local, fk.plugin), sig);
        }
    }
    out
}

/// Drop every (ONAM, DATA) pair in a SCOL `record` whose ONAM resolves to a SCOL
/// target — which is valid in FO76 (nested SCOLs) but illegal in FO4.
///
/// Paired-array lockstep rule: every ONAM is immediately followed by exactly one
/// DATA within the `scope_id='parts'` group. Dropping ONAM alone would orphan the
/// DATA (xEdit "out of order" error / potential CTD). The drain-and-rebuild walk
/// sets a `drop_next_data` flag when it removes an ONAM so the immediately
/// following DATA is also discarded.
///
/// "Positively wrong type" ONAMs (resolved SCOL) and NULL ONAMs are acted on. A
/// dangling ONAM (non-null, unresolvable) is left for the sweep / invalid-target
/// fixups, matching the conservative policy of `apply_to_record`.
///
/// Returns the number of ONAM+DATA pairs dropped.
pub fn strip_scol_wrong_type_onam_data_pairs(
    record: &mut Record,
    fk_to_sig: &FxHashMap<(u32, Sym), SigCode>,
    master_resolved: &FxHashMap<(u32, Sym), SigCode>,
) -> u32 {
    let (Ok(onam_sub), Ok(data_sub)) = (
        SubrecordSig::from_str("ONAM"),
        SubrecordSig::from_str("DATA"),
    ) else {
        return 0;
    };

    let mut acted = 0u32;
    let mut drop_next_data = false;
    let mut new_fields: smallvec::SmallVec<[crate::record::FieldEntry; 8]> =
        smallvec::SmallVec::new();

    for entry in record.fields.drain(..) {
        // Consume the DATA that was paired with the just-dropped ONAM.
        if drop_next_data && entry.sig == data_sub {
            drop_next_data = false;
            // drop this DATA — do not push it
            continue;
        }
        // If drop_next_data is still set but we see something OTHER than DATA,
        // reset the flag — the paired DATA is missing (malformed record). Keep the
        // unexpected subrecord as-is; the validator will surface any structural issue.
        drop_next_data = false;

        if entry.sig == onam_sub {
            let resolved_sig = first_formkey(&entry.value).and_then(|fk| {
                fk_to_sig
                    .get(&(fk.local, fk.plugin))
                    .or_else(|| master_resolved.get(&(fk.local, fk.plugin)))
            });
            if scol_onam_is_null(&entry.value)
                || resolved_sig.is_some_and(|sig| sig.as_str() == "SCOL")
            {
                // Wrong type: schedule the next DATA for removal too.
                acted += 1;
                drop_next_data = true;
                continue; // drop this ONAM
            }
        }

        new_fields.push(entry);
    }

    // Always reassign: drain(..) empties the original regardless of `acted`.
    record.fields = new_fields;
    acted
}

fn scol_onam_is_null(value: &FieldValue) -> bool {
    match value {
        FieldValue::None => true,
        FieldValue::FormKey(fk) => fk.local == 0,
        FieldValue::Uint(n) => (*n as u32) & 0x00FF_FFFF == 0,
        FieldValue::Int(n) if *n >= 0 => (*n as u32) & 0x00FF_FFFF == 0,
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            read_u32_le(bytes, 0).is_some_and(|raw| raw & 0x00FF_FFFF == 0)
        }
        _ => false,
    }
}

fn strip_scol_mnam_subrecords(record: &mut Record) -> u32 {
    let Ok(mnam_sub) = SubrecordSig::from_str("MNAM") else {
        return 0;
    };
    let before = record.fields.len();
    record.fields.retain(|entry| entry.sig != mnam_sub);
    (before - record.fields.len()) as u32
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, Record, RecordFlags};
    use crate::sym::StringInterner;

    fn make_fk(local: u32, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    fn make_record(sig: &str, fields: Vec<FieldEntry>, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: make_fk(0x000800, "Out.esp", interner),
            eid: None,
            flags: RecordFlags::empty(),
            fields: fields.into_iter().collect(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn fk_field(sub: &str, fk: FormKey) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sub).unwrap(),
            value: FieldValue::FormKey(fk),
        }
    }

    fn bytes_field(sub: &str, bytes: Vec<u8>) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sub).unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::from_vec(bytes)),
        }
    }

    fn set_u32(bytes: &mut [u8], off: usize, value: u32) {
        bytes[off..off + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn field_bytes(record: &Record, sub: &str) -> Vec<u8> {
        let sub_sig = SubrecordSig::from_str(sub).unwrap();
        let entry = record
            .fields
            .iter()
            .find(|entry| entry.sig == sub_sig)
            .unwrap();
        let FieldValue::Bytes(bytes) = &entry.value else {
            panic!("expected Bytes field");
        };
        bytes.to_vec()
    }

    fn sig_map(
        entries: &[(u32, &str, &str)],
        interner: &StringInterner,
    ) -> FxHashMap<(u32, Sym), SigCode> {
        let mut m = FxHashMap::default();
        for (local, plugin, sig) in entries {
            m.insert(
                (*local, interner.intern(plugin)),
                SigCode::from_str(sig).unwrap(),
            );
        }
        m
    }

    /// Stand-in for the schema's `allows_target`: only `allowed` sigs pass.
    fn allows_only(allowed: &'static [&'static str]) -> impl Fn(&str) -> bool {
        move |sig: &str| allowed.contains(&sig)
    }

    #[test]
    fn strips_fact_venc_when_target_is_not_refr() {
        let interner = StringInterner::new();
        let venc_fk = make_fk(0x001000, "Out.esp", &interner);
        let record_fk = make_fk(0x000800, "Out.esp", &interner);
        let edid = FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(interner.intern("TestFACT")),
        };
        let mut record = make_record("FACT", vec![edid, fk_field("VENC", venc_fk)], &interner);
        let _ = record_fk;
        let map = sig_map(&[(0x001000, "Out.esp", "KYWD")], &interner);

        let outcome = apply_to_record(
            &mut record,
            SubrecordSig::from_str("VENC").unwrap(),
            Action::Strip,
            false,
            &allows_only(&["REFR"]),
            &map,
            &FxHashMap::default(),
        );

        assert!(outcome.changed);
        assert_eq!(outcome.acted, 1);
        assert_eq!(record.fields.len(), 1, "VENC stripped, EDID kept");
        assert_eq!(record.fields[0].sig.as_str(), "EDID");
    }

    #[test]
    fn keeps_fact_venc_when_target_is_refr() {
        let interner = StringInterner::new();
        let venc_fk = make_fk(0x001000, "Out.esp", &interner);
        let mut record = make_record("FACT", vec![fk_field("VENC", venc_fk)], &interner);
        let map = sig_map(&[(0x001000, "Out.esp", "REFR")], &interner);

        let outcome = apply_to_record(
            &mut record,
            SubrecordSig::from_str("VENC").unwrap(),
            Action::Strip,
            false,
            &allows_only(&["REFR"]),
            &map,
            &FxHashMap::default(),
        );

        assert!(!outcome.changed, "REFR target is legal — keep VENC");
        assert_eq!(record.fields.len(), 1);
    }

    #[test]
    fn nulls_dial_bnam_when_target_is_not_dlbr() {
        let interner = StringInterner::new();
        let bnam_fk = make_fk(0x001000, "Out.esp", &interner);
        let mut record = make_record("DIAL", vec![fk_field("BNAM", bnam_fk)], &interner);
        let map = sig_map(&[(0x001000, "Out.esp", "QUST")], &interner);

        let outcome = apply_to_record(
            &mut record,
            SubrecordSig::from_str("BNAM").unwrap(),
            Action::Null,
            false,
            &allows_only(&["DLBR"]),
            &map,
            &FxHashMap::default(),
        );

        assert!(outcome.changed);
        assert_eq!(outcome.acted, 1);
        assert_eq!(record.fields.len(), 1, "BNAM kept, FK nulled");
        let FieldValue::FormKey(fk) = &record.fields[0].value else {
            panic!("expected FormKey");
        };
        assert_eq!(fk.local, 0);
    }

    #[test]
    fn leaves_dangling_ref_untouched() {
        let interner = StringInterner::new();
        let bnam_fk = make_fk(0x00ABCD, "Out.esp", &interner);
        let mut record = make_record("DIAL", vec![fk_field("BNAM", bnam_fk)], &interner);
        // FK not in the map → unresolved/dangling.
        let map = sig_map(&[], &interner);

        let outcome = apply_to_record(
            &mut record,
            SubrecordSig::from_str("BNAM").unwrap(),
            Action::Null,
            false,
            &allows_only(&["DLBR"]),
            &map,
            &FxHashMap::default(),
        );

        assert!(!outcome.changed, "dangling refs are not ours to act on");
        let FieldValue::FormKey(fk) = &record.fields[0].value else {
            panic!("expected FormKey");
        };
        assert_eq!(fk.local, 0x00ABCD, "FK left intact");
    }

    #[test]
    fn strips_dangling_xrfg_when_strip_dangling_set() {
        // K/O root: an XRFG (→RFGP) whose remapped target resolves NOWHERE — the
        // FO76 RFGP has no FO4 home. With strip_dangling, the slot is dropped
        // rather than left (it can never point at a valid record).
        let interner = StringInterner::new();
        let xrfg_fk = make_fk(0x01004A6C, "Out.esp", &interner); // 01xxxxxx, unresolved
        let mut record = make_record("REFR", vec![fk_field("XRFG", xrfg_fk)], &interner);
        let map = sig_map(&[], &interner); // resolves nowhere

        let outcome = apply_to_record(
            &mut record,
            SubrecordSig::from_str("XRFG").unwrap(),
            Action::Strip,
            true, // strip_dangling
            &allows_only(&["RFGP"]),
            &map,
            &FxHashMap::default(),
        );

        assert!(outcome.changed);
        assert_eq!(outcome.acted, 1);
        assert!(record.fields.is_empty(), "dangling XRFG stripped");
    }

    #[test]
    fn strips_wrong_type_xrfg_pointing_at_output_collision() {
        // K root (collision): XRFG remapped to a 07 object-id that landed on an
        // unrelated output REFR (xEdit "Found a REFR, expected: RFGP"). Stripped.
        let interner = StringInterner::new();
        let xrfg_fk = make_fk(0x07662FB2, "Out.esp", &interner);
        let mut record = make_record("REFR", vec![fk_field("XRFG", xrfg_fk)], &interner);
        let map = sig_map(&[(0x07662FB2, "Out.esp", "REFR")], &interner);

        let outcome = apply_to_record(
            &mut record,
            SubrecordSig::from_str("XRFG").unwrap(),
            Action::Strip,
            true,
            &allows_only(&["RFGP"]),
            &map,
            &FxHashMap::default(),
        );

        assert!(outcome.changed);
        assert_eq!(outcome.acted, 1);
        assert!(record.fields.is_empty(), "wrong-type XRFG stripped");
    }

    #[test]
    fn retargets_info_speaker_vtyp_to_unique_npc_using_that_voice_type() {
        let interner = StringInterner::new();
        let vtyp_fk = make_fk(0x2AAD63, "Out.esp", &interner);
        let npc_fk = make_fk(0x3B91C1, "Out.esp", &interner);
        let mut record = make_record("INFO", vec![fk_field("ANAM", vtyp_fk)], &interner);
        let mut voice_type_npcs = FxHashMap::default();
        voice_type_npcs.insert((vtyp_fk.local, vtyp_fk.plugin), vec![npc_fk]);

        let outcome = retarget_or_strip_info_speaker(
            &mut record,
            SubrecordSig::from_str("ANAM").unwrap(),
            Some(SigCode::from_str("VTYP").unwrap()),
            &voice_type_npcs,
        );

        assert!(outcome.changed);
        assert_eq!(outcome.acted, 1);
        let FieldValue::FormKey(actual) = &record.fields[0].value else {
            panic!("expected retargeted speaker FormKey");
        };
        assert_eq!(*actual, npc_fk);
    }

    #[test]
    fn synthesized_info_speaker_proxy_uses_requested_voice_and_drops_template_script() {
        let interner = StringInterner::new();
        let template_voice = make_fk(0x00002D, "Fallout4.esm", &interner);
        let requested_voice = make_fk(0x590FB9, "SeventySix.esm", &interner);
        let proxy_fk = make_fk(0x800123, "SeventySix.esm", &interner);
        let template = make_record(
            "NPC_",
            vec![
                FieldEntry {
                    sig: SubrecordSig::from_str("EDID").unwrap(),
                    value: FieldValue::String(interner.intern("TemplateNPC")),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("VMAD").unwrap(),
                    value: FieldValue::Bytes(smallvec::SmallVec::from_slice(b"script")),
                },
                fk_field("VTCK", template_voice),
                bytes_field("DATA", vec![0; 24]),
            ],
            &interner,
        );

        let proxy =
            build_info_speaker_proxy(&template, proxy_fk, requested_voice, &interner).unwrap();

        assert_eq!(proxy.form_key, proxy_fk);
        assert_eq!(proxy.flags, RecordFlags::empty());
        assert!(
            proxy
                .fields
                .iter()
                .all(|entry| entry.sig.as_str() != "VMAD")
        );
        let eid = proxy.eid.and_then(|sym| interner.resolve(sym)).unwrap();
        assert_eq!(eid, "BACUP_InfoSpeaker_SeventySix_esm_590FB9");
        let voice = proxy
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "VTCK")
            .and_then(|entry| first_formkey(&entry.value));
        assert_eq!(voice, Some(requested_voice));
    }

    #[test]
    fn synthesized_info_speaker_allocation_keys_distinguish_source_plugins() {
        let interner = StringInterner::new();
        let first = synthetic_info_speaker_source_key(
            make_fk(0x001234, "SourceA.esm", &interner),
            &interner,
        );
        let second = synthetic_info_speaker_source_key(
            make_fk(0x001234, "SourceB.esm", &interner),
            &interner,
        );

        assert_ne!(first.plugin, second.plugin);
        assert_eq!(first.local, second.local);
    }

    #[test]
    fn synthesized_info_speaker_prefers_small_non_templated_npc() {
        let interner = StringInterner::new();
        let voice = make_fk(0x00002D, "Fallout4.esm", &interner);
        let templated = make_record(
            "NPC_",
            vec![
                fk_field("VTCK", voice),
                fk_field("TPLT", make_fk(0x123, "Fallout4.esm", &interner)),
            ],
            &interner,
        );
        let direct = make_record(
            "NPC_",
            vec![
                fk_field("VTCK", voice),
                bytes_field("DATA", vec![0; 24]),
                bytes_field("DNAM", vec![0; 8]),
            ],
            &interner,
        );

        assert!(prefer_info_speaker_template(&direct, &templated, &interner));
        assert!(!prefer_info_speaker_template(
            &templated, &direct, &interner
        ));
    }

    #[test]
    fn retargets_info_speaker_vtyp_to_lowest_npc_when_multiple_use_that_voice_type() {
        let interner = StringInterner::new();
        let vtyp_fk = make_fk(0x4E4A12, "Out.esp", &interner);
        let first_npc_fk = make_fk(0x1827F9, "Out.esp", &interner);
        let second_npc_fk = make_fk(0x42AF7E, "Out.esp", &interner);
        let mut record = make_record("INFO", vec![fk_field("ANAM", vtyp_fk)], &interner);
        let mut voice_type_npcs = FxHashMap::default();
        voice_type_npcs.insert(
            (vtyp_fk.local, vtyp_fk.plugin),
            vec![second_npc_fk, first_npc_fk],
        );

        let outcome = retarget_or_strip_info_speaker(
            &mut record,
            SubrecordSig::from_str("ANAM").unwrap(),
            Some(SigCode::from_str("VTYP").unwrap()),
            &voice_type_npcs,
        );

        assert!(outcome.changed);
        assert_eq!(outcome.acted, 1);
        let FieldValue::FormKey(actual) = &record.fields[0].value else {
            panic!("expected retargeted speaker FormKey");
        };
        assert_eq!(*actual, first_npc_fk);
    }

    #[test]
    fn strips_info_speaker_vtyp_when_no_npc_uses_that_voice_type() {
        let interner = StringInterner::new();
        let vtyp_fk = make_fk(0x2AAD63, "Out.esp", &interner);
        let mut record = make_record("INFO", vec![fk_field("ANAM", vtyp_fk)], &interner);
        let voice_type_npcs = FxHashMap::default();

        let outcome = retarget_or_strip_info_speaker(
            &mut record,
            SubrecordSig::from_str("ANAM").unwrap(),
            Some(SigCode::from_str("VTYP").unwrap()),
            &voice_type_npcs,
        );

        assert!(outcome.changed);
        assert_eq!(outcome.acted, 1);
        assert!(
            record.fields.is_empty(),
            "unretargetable VTYP speaker is stripped"
        );
    }

    #[test]
    fn keeps_info_speaker_that_already_resolves_to_npc() {
        let interner = StringInterner::new();
        let npc_fk = make_fk(0x3B91C1, "Out.esp", &interner);
        let mut record = make_record("INFO", vec![fk_field("ANAM", npc_fk)], &interner);
        let voice_type_npcs = FxHashMap::default();

        let outcome = retarget_or_strip_info_speaker(
            &mut record,
            SubrecordSig::from_str("ANAM").unwrap(),
            Some(SigCode::from_str("NPC_").unwrap()),
            &voice_type_npcs,
        );

        assert!(!outcome.changed);
        assert_eq!(record.fields.len(), 1);
    }

    #[test]
    fn strips_info_speaker_master_vtyp_when_unresolvable_in_output() {
        // Class E core: the ANAM speaker points at a MASTER VTYP (e.g.
        // Fallout4.esm VTYP:0000002D). The output-only fk_to_sig misses it, but
        // resolve_speaker_sig recovers "VTYP" from the master handle, so the
        // wrong-type strip fires (no NPC uses the voice type → strip).
        let interner = StringInterner::new();
        let vtyp_fk = make_fk(0x00002D, "Fallout4.esm", &interner);
        let mut record = make_record("INFO", vec![fk_field("ANAM", vtyp_fk)], &interner);
        let voice_type_npcs = FxHashMap::default();

        let outcome = retarget_or_strip_info_speaker(
            &mut record,
            SubrecordSig::from_str("ANAM").unwrap(),
            Some(SigCode::from_str("VTYP").unwrap()),
            &voice_type_npcs,
        );

        assert!(outcome.changed);
        assert_eq!(outcome.acted, 1);
        assert!(record.fields.is_empty(), "master VTYP speaker stripped");
    }

    #[test]
    fn unconstrained_allows_predicate_is_no_op() {
        // An `allows` that accepts every sig (the schema returned no constraint /
        // an empty target set) must never act.
        let interner = StringInterner::new();
        let venc_fk = make_fk(0x001000, "Out.esp", &interner);
        let mut record = make_record("FACT", vec![fk_field("VENC", venc_fk)], &interner);
        let map = sig_map(&[(0x001000, "Out.esp", "KYWD")], &interner);

        let outcome = apply_to_record(
            &mut record,
            SubrecordSig::from_str("VENC").unwrap(),
            Action::Strip,
            false,
            &|_sig: &str| true,
            &map,
            &FxHashMap::default(),
        );

        assert!(!outcome.changed, "unconstrained field must be a no-op");
    }

    #[test]
    fn strips_already_null_fk_in_strip_action_subrecord() {
        // The (d) fix: an upstream-nulled FK in an optional NULL-disallowed
        // subrecord is "Found NULL, expected X" — strip the whole subrecord so
        // it's absent (valid) rather than present-but-NULL.
        let interner = StringInterner::new();
        let null_fk = make_fk(0, "Out.esp", &interner);
        let mut record = make_record("DIAL", vec![fk_field("BNAM", null_fk)], &interner);
        let map = sig_map(&[], &interner);

        let outcome = apply_to_record(
            &mut record,
            SubrecordSig::from_str("BNAM").unwrap(),
            Action::Strip,
            false,
            &allows_only(&["DLBR"]),
            &map,
            &FxHashMap::default(),
        );

        assert!(outcome.changed);
        assert_eq!(outcome.acted, 1);
        assert!(
            record.fields.is_empty(),
            "null BNAM must be stripped, not kept"
        );
    }

    #[test]
    fn strips_present_but_none_decoded_subrecord_in_strip_action() {
        // A present-but-null `formid` subrecord decodes to FieldValue::None
        // (source_read: raw==0 => None), so first_formkey finds no FK, yet xEdit
        // still reports "Found NULL, expected X". A Strip action must drop it.
        let interner = StringInterner::new();
        let bnam_none = FieldEntry {
            sig: SubrecordSig::from_str("BNAM").unwrap(),
            value: FieldValue::None,
        };
        let mut record = make_record("DIAL", vec![bnam_none], &interner);
        let map = sig_map(&[], &interner);

        let outcome = apply_to_record(
            &mut record,
            SubrecordSig::from_str("BNAM").unwrap(),
            Action::Strip,
            false,
            &allows_only(&["DLBR"]),
            &map,
            &FxHashMap::default(),
        );

        assert!(outcome.changed);
        assert_eq!(outcome.acted, 1);
        assert!(
            record.fields.is_empty(),
            "present-but-None BNAM must be stripped, not kept"
        );
    }

    #[test]
    fn leaves_present_but_none_subrecord_for_null_action() {
        // A None-decoded subrecord under a Null action (NULL is allowed) is fine
        // as-is — don't strip it.
        let interner = StringInterner::new();
        let bnam_none = FieldEntry {
            sig: SubrecordSig::from_str("BNAM").unwrap(),
            value: FieldValue::None,
        };
        let mut record = make_record("DIAL", vec![bnam_none], &interner);
        let map = sig_map(&[], &interner);

        let outcome = apply_to_record(
            &mut record,
            SubrecordSig::from_str("BNAM").unwrap(),
            Action::Null,
            false,
            &allows_only(&["DLBR"]),
            &map,
            &FxHashMap::default(),
        );

        assert!(!outcome.changed);
        assert_eq!(record.fields.len(), 1);
    }

    #[test]
    fn null_action_keeps_already_null_fk() {
        // A Null-action field that's already null is as-good-as-it-gets — no-op.
        let interner = StringInterner::new();
        let null_fk = make_fk(0, "Out.esp", &interner);
        let mut record = make_record("DIAL", vec![fk_field("BNAM", null_fk)], &interner);
        let map = sig_map(&[], &interner);

        let outcome = apply_to_record(
            &mut record,
            SubrecordSig::from_str("BNAM").unwrap(),
            Action::Null,
            false,
            &allows_only(&["DLBR"]),
            &map,
            &FxHashMap::default(),
        );

        assert!(!outcome.changed);
        assert_eq!(record.fields.len(), 1);
    }

    #[test]
    fn action_for_derives_metadata_driven_action() {
        // NULL allowed → Null; NULL disallowed + optional → Strip; NULL
        // disallowed + required → Leave.
        assert_eq!(action_for(true, false), Action::Null);
        assert_eq!(action_for(true, true), Action::Null);
        assert_eq!(action_for(false, false), Action::Strip);
        assert_eq!(action_for(false, true), Action::Leave);
    }

    #[test]
    fn encoded_sigs_reuse_fk_index_without_fallback() {
        let interner = StringInterner::new();
        let fk_to_sig = sig_map(
            &[
                (0x001234, "fallout4.ESM", "STAT"),
                (0x005678, "Out.esp", "PROJ"),
            ],
            &interner,
        );
        let encoded_sigs = encoded_sigs_from_fk_index_or_else(
            &fk_to_sig,
            &interner,
            &["Fallout4.esm".to_string()],
            || -> Result<FxHashMap<u32, SigCode>, ()> { panic!("unexpected fallback") },
        )
        .unwrap();

        assert!(raw_resolves_to_sig(
            0x00001234,
            "STAT",
            &encoded_sigs,
            &FxHashMap::default(),
        ));
        assert!(raw_resolves_to_sig(
            0x01005678,
            "PROJ",
            &encoded_sigs,
            &FxHashMap::default(),
        ));
    }

    #[test]
    fn encoded_sig_collision_uses_fallback_index() {
        let interner = StringInterner::new();
        let fk_to_sig = sig_map(
            &[
                (0x001234, "Out.esp", "STAT"),
                (0x001234, "Other.esp", "PROJ"),
            ],
            &interner,
        );
        let mut fallback_called = false;
        let encoded_sigs = encoded_sigs_from_fk_index_or_else(
            &fk_to_sig,
            &interner,
            &["Fallout4.esm".to_string()],
            || {
                fallback_called = true;
                Ok::<_, ()>(FxHashMap::from_iter([(
                    0x01001234,
                    SigCode::from_str("PROJ").unwrap(),
                )]))
            },
        )
        .unwrap();

        assert!(fallback_called);
        assert!(raw_resolves_to_sig(
            0x01001234,
            "PROJ",
            &encoded_sigs,
            &FxHashMap::default(),
        ));
    }

    #[test]
    fn nulls_unresolved_mgef_primary_actor_value() {
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let actor_value_off =
            struct_field_offset(&schema, "MGEF", "DATA", Some(131), "actor_value").unwrap();
        let actor_value_1_off =
            struct_field_offset(&schema, "MGEF", "DATA", Some(131), "actor_value_1").unwrap();
        let mut data = vec![0; 96];
        set_u32(&mut data, actor_value_off, 0x00000397);
        set_u32(&mut data, actor_value_1_off, 0x07123101);
        let mut record = make_record("MGEF", vec![bytes_field("DATA", data)], &interner);
        let mut encoded_sigs = FxHashMap::default();
        encoded_sigs.insert(0x07123101, SigCode::from_str("AVIF").unwrap());

        let nulled = null_invalid_mgef_actor_values(
            &mut record,
            &schema,
            Some(131),
            &encoded_sigs,
            &FxHashMap::default(),
            &["Fallout4.esm".to_string()],
        );

        let data = field_bytes(&record, "DATA");
        assert_eq!(nulled, 1);
        assert_eq!(read_u32_le(&data, actor_value_off), Some(0));
        assert_eq!(read_u32_le(&data, actor_value_1_off), Some(0x07123101));
    }

    #[test]
    fn keeps_fo4_hardcoded_mgef_actor_value() {
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let actor_value_off =
            struct_field_offset(&schema, "MGEF", "DATA", Some(131), "actor_value").unwrap();
        let mut data = vec![0; 96];
        set_u32(&mut data, actor_value_off, 0x000002D2);
        let mut record = make_record("MGEF", vec![bytes_field("DATA", data)], &interner);

        let nulled = null_invalid_mgef_actor_values(
            &mut record,
            &schema,
            Some(131),
            &FxHashMap::default(),
            &FxHashMap::default(),
            &["Fallout4.esm".to_string()],
        );

        let data = field_bytes(&record, "DATA");
        assert_eq!(nulled, 0);
        assert_eq!(read_u32_le(&data, actor_value_off), Some(0x000002D2));
    }

    #[test]
    fn normalizes_aimed_mgef_when_projectile_is_missing() {
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let delivery_off =
            struct_field_offset(&schema, "MGEF", "DATA", Some(131), "delivery").unwrap();
        let mut data = vec![0; 96];
        set_u32(&mut data, delivery_off, MAGIC_TARGET_AIMED);
        let mut record = make_record("MGEF", vec![bytes_field("DATA", data)], &interner);

        let acted = normalize_aimed_mgef_without_projectile(
            &mut record,
            &schema,
            Some(131),
            &FxHashMap::default(),
            &FxHashMap::default(),
        );

        let data = field_bytes(&record, "DATA");
        assert_eq!(acted, 1);
        assert_eq!(read_u32_le(&data, delivery_off), Some(MAGIC_TARGET_SELF));
    }

    #[test]
    fn keeps_aimed_mgef_when_projectile_resolves_to_proj() {
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let delivery_off =
            struct_field_offset(&schema, "MGEF", "DATA", Some(131), "delivery").unwrap();
        let projectile_off =
            struct_field_offset(&schema, "MGEF", "DATA", Some(131), "projectile").unwrap();
        let mut data = vec![0; 96];
        set_u32(&mut data, delivery_off, MAGIC_TARGET_AIMED);
        set_u32(&mut data, projectile_off, 0x07101010);
        let mut record = make_record("MGEF", vec![bytes_field("DATA", data)], &interner);
        let mut encoded_sigs = FxHashMap::default();
        encoded_sigs.insert(0x07101010, SigCode::from_str("PROJ").unwrap());

        let acted = normalize_aimed_mgef_without_projectile(
            &mut record,
            &schema,
            Some(131),
            &encoded_sigs,
            &FxHashMap::default(),
        );

        let data = field_bytes(&record, "DATA");
        assert_eq!(acted, 0);
        assert_eq!(read_u32_le(&data, delivery_off), Some(MAGIC_TARGET_AIMED));
    }

    #[test]
    fn normalizes_aimed_spell_when_effects_have_no_projectile() {
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let effect = make_fk(0x001234, "Out.esp", &interner);
        let target_type_off =
            struct_field_offset(&schema, "SPEL", "SPIT", Some(131), "target_type").unwrap();
        let mut spit = vec![0; 36];
        set_u32(&mut spit, target_type_off, MAGIC_TARGET_AIMED);
        let mut record = make_record(
            "SPEL",
            vec![bytes_field("SPIT", spit), fk_field("EFID", effect)],
            &interner,
        );
        let mut mgef_projectiles = FxHashMap::default();
        mgef_projectiles.insert((effect.local, effect.plugin), false);

        let acted = normalize_aimed_spell_without_projectile_effects(
            &mut record,
            &schema,
            Some(131),
            &mgef_projectiles,
        );

        let spit = field_bytes(&record, "SPIT");
        assert_eq!(acted, 1);
        assert_eq!(read_u32_le(&spit, target_type_off), Some(MAGIC_TARGET_SELF));
    }

    #[test]
    fn keeps_aimed_spell_when_any_effect_has_projectile() {
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let effect = make_fk(0x001234, "Out.esp", &interner);
        let target_type_off =
            struct_field_offset(&schema, "SPEL", "SPIT", Some(131), "target_type").unwrap();
        let mut spit = vec![0; 36];
        set_u32(&mut spit, target_type_off, MAGIC_TARGET_AIMED);
        let mut record = make_record(
            "SPEL",
            vec![bytes_field("SPIT", spit), fk_field("EFID", effect)],
            &interner,
        );
        let mut mgef_projectiles = FxHashMap::default();
        mgef_projectiles.insert((effect.local, effect.plugin), true);

        let acted = normalize_aimed_spell_without_projectile_effects(
            &mut record,
            &schema,
            Some(131),
            &mgef_projectiles,
        );

        let spit = field_bytes(&record, "SPIT");
        assert_eq!(acted, 0);
        assert_eq!(
            read_u32_le(&spit, target_type_off),
            Some(MAGIC_TARGET_AIMED)
        );
    }

    #[test]
    fn leaves_aimed_spell_when_effect_projectile_state_is_unknown() {
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let effect = make_fk(0x001234, "Fallout4.esm", &interner);
        let target_type_off =
            struct_field_offset(&schema, "SPEL", "SPIT", Some(131), "target_type").unwrap();
        let mut spit = vec![0; 36];
        set_u32(&mut spit, target_type_off, MAGIC_TARGET_AIMED);
        let mut record = make_record(
            "SPEL",
            vec![bytes_field("SPIT", spit), fk_field("EFID", effect)],
            &interner,
        );

        let acted = normalize_aimed_spell_without_projectile_effects(
            &mut record,
            &schema,
            Some(131),
            &FxHashMap::default(),
        );

        let spit = field_bytes(&record, "SPIT");
        assert_eq!(acted, 0);
        assert_eq!(
            read_u32_le(&spit, target_type_off),
            Some(MAGIC_TARGET_AIMED)
        );
    }

    #[test]
    fn nulls_cobj_cnam_when_target_is_pkin() {
        // FO76 COBJ created-object points at a PKIN (no FO4 equivalent). CNAM is
        // a nullable single-FK subrecord → Null action nulls the FK in place.
        let interner = StringInterner::new();
        let cnam_fk = make_fk(0x005DD312, "Out.esp", &interner);
        let mut record = make_record("COBJ", vec![fk_field("CNAM", cnam_fk)], &interner);
        let map = sig_map(&[(0x005DD312, "Out.esp", "PKIN")], &interner);

        let outcome = apply_to_record(
            &mut record,
            SubrecordSig::from_str("CNAM").unwrap(),
            Action::Null,
            false,
            &allows_only(&["MISC", "WEAP", "ARMO"]),
            &map,
            &FxHashMap::default(),
        );

        assert!(outcome.changed);
        assert_eq!(outcome.acted, 1);
        assert_eq!(record.fields.len(), 1, "CNAM kept, FK nulled");
        let FieldValue::FormKey(fk) = &record.fields[0].value else {
            panic!("expected FormKey");
        };
        assert_eq!(fk.local, 0, "PKIN created-object FK nulled");
    }

    #[test]
    fn keeps_cobj_cnam_when_target_is_valid_misc() {
        let interner = StringInterner::new();
        let cnam_fk = make_fk(0x001000, "Out.esp", &interner);
        let mut record = make_record("COBJ", vec![fk_field("CNAM", cnam_fk)], &interner);
        let map = sig_map(&[(0x001000, "Out.esp", "MISC")], &interner);

        let outcome = apply_to_record(
            &mut record,
            SubrecordSig::from_str("CNAM").unwrap(),
            Action::Null,
            false,
            &allows_only(&["MISC", "WEAP", "ARMO"]),
            &map,
            &FxHashMap::default(),
        );

        assert!(!outcome.changed, "MISC is a legal created-object — keep");
        let FieldValue::FormKey(fk) = &record.fields[0].value else {
            panic!("expected FormKey");
        };
        assert_eq!(fk.local, 0x001000);
    }

    fn kw_list_field(sub: &str, fks: &[FormKey]) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sub).unwrap(),
            value: FieldValue::List(fks.iter().map(|fk| FieldValue::FormKey(*fk)).collect()),
        }
    }

    #[test]
    fn filters_only_wrong_type_keyword_list_entries() {
        let interner = StringInterner::new();
        let good = make_fk(0x0037D0B2, "Out.esp", &interner); // KYWD
        let bad = make_fk(0x00033B61, "Out.esp", &interner); // SCOL (wrong)
        let mut record = make_record("OMOD", vec![kw_list_field("MNAM", &[good, bad])], &interner);
        let map = sig_map(
            &[
                (0x0037D0B2, "Out.esp", "KYWD"),
                (0x00033B61, "Out.esp", "SCOL"),
            ],
            &interner,
        );

        let removed = filter_keyword_list_entries(
            &mut record,
            SubrecordSig::from_str("MNAM").unwrap(),
            &allows_only(&["KYWD"]),
            &map,
            &FxHashMap::default(),
        );

        assert_eq!(removed, 1, "only the SCOL entry is dropped");
        assert_eq!(record.fields.len(), 1, "MNAM kept (still has the KYWD)");
        let FieldValue::List(items) = &record.fields[0].value else {
            panic!("expected List");
        };
        assert_eq!(items.len(), 1, "the valid KYWD entry survives");
        let FieldValue::FormKey(fk) = &items[0] else {
            panic!("expected FormKey");
        };
        assert_eq!(fk.local, 0x0037D0B2);
    }

    #[test]
    fn drops_keyword_list_subrecord_when_all_entries_wrong_type() {
        let interner = StringInterner::new();
        let bad = make_fk(0x00033B61, "Out.esp", &interner); // SCOL
        let mut record = make_record("OMOD", vec![kw_list_field("MNAM", &[bad])], &interner);
        let map = sig_map(&[(0x00033B61, "Out.esp", "SCOL")], &interner);

        let removed = filter_keyword_list_entries(
            &mut record,
            SubrecordSig::from_str("MNAM").unwrap(),
            &allows_only(&["KYWD"]),
            &map,
            &FxHashMap::default(),
        );

        assert_eq!(removed, 1);
        assert!(record.fields.is_empty(), "emptied MNAM is dropped entirely");
    }

    #[test]
    fn keeps_dangling_and_null_keyword_list_entries() {
        let interner = StringInterner::new();
        let dangling = make_fk(0x00ABCDEF, "Out.esp", &interner); // not in map
        let null_fk = make_fk(0, "Out.esp", &interner);
        let mut record = make_record(
            "OMOD",
            vec![kw_list_field("MNAM", &[dangling, null_fk])],
            &interner,
        );
        let map = sig_map(&[], &interner); // nothing resolves

        let removed = filter_keyword_list_entries(
            &mut record,
            SubrecordSig::from_str("MNAM").unwrap(),
            &allows_only(&["KYWD"]),
            &map,
            &FxHashMap::default(),
        );

        assert_eq!(
            removed, 0,
            "dangling + null entries are left for sweep fixups"
        );
        let FieldValue::List(items) = &record.fields[0].value else {
            panic!("expected List");
        };
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn encode_target_fk_returns_none_for_plugin_absent_from_masters() {
        let interner = StringInterner::new();
        let masters = vec!["Fallout4.esm".to_string(), "DLCNukaWorld.esm".to_string()];

        // Present master → encoded with its load index.
        let present = make_fk(0x033B61, "DLCNukaWorld.esm", &interner);
        assert_eq!(
            encode_target_fk(&present, &interner, &masters),
            Some((1u32 << 24) | 0x033B61)
        );

        // Absent plugin → None, NOT the `masters.len()` sentinel bucket. Falling
        // into that bucket would collide with an output own-record at the same
        // object id (a SCOL) in `output_encoded_sigs` and wrongly drop the ref.
        let absent = make_fk(0x033B61, "DLCNukaWorld.esm", &interner);
        let masters_without_dlc = vec!["Fallout4.esm".to_string()];
        assert_eq!(
            encode_target_fk(&absent, &interner, &masters_without_dlc),
            None
        );

        // Null FK is always the null encoding regardless of the master list.
        let null_fk = make_fk(0, "DLCNukaWorld.esm", &interner);
        assert_eq!(encode_target_fk(&null_fk, &interner, &masters), Some(0));
    }

    fn ctda(function_id: u16, parameter_1: u32) -> Vec<u8> {
        let mut raw = vec![0u8; 28];
        raw[8..10].copy_from_slice(&function_id.to_le_bytes());
        raw[12..16].copy_from_slice(&parameter_1.to_le_bytes());
        raw
    }

    #[test]
    fn drops_null_required_param1_ctda_with_script_strings() {
        let interner = StringInterner::new();
        let mut record = make_record(
            "SNDR",
            vec![
                bytes_field("CTDA", ctda(248, 0)),
                bytes_field("CIS1", b"scene\0".to_vec()),
                bytes_field("BNAM", vec![1, 2, 3, 4, 5, 6]),
            ],
            &interner,
        );

        let dropped = drop_null_required_param1_conditions(&mut record);

        assert_eq!(dropped, 1);
        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect();
        assert_eq!(sigs, vec!["BNAM"], "CTDA and trailing CIS1 dropped");
    }

    #[test]
    fn keeps_required_param1_ctda_when_parameter_is_present() {
        let interner = StringInterner::new();
        let mut record = make_record(
            "ACTI",
            vec![bytes_field("CTDA", ctda(14, 0x000002C3))],
            &interner,
        );

        let dropped = drop_null_required_param1_conditions(&mut record);

        assert_eq!(dropped, 0);
        assert_eq!(record.fields.len(), 1);
    }

    #[test]
    fn drops_orphan_alch_efit_without_base_effect() {
        let interner = StringInterner::new();
        let effect = make_fk(0x0011D53C, "Fallout4.esm", &interner);
        let mut record = make_record(
            "ALCH",
            vec![
                bytes_field("EFIT", vec![1; 12]),
                fk_field("EFID", effect),
                bytes_field("EFIT", vec![2; 12]),
            ],
            &interner,
        );

        let dropped = drop_orphan_effect_data_fields(&mut record);

        assert_eq!(dropped, 1);
        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect();
        assert_eq!(sigs, vec!["EFID", "EFIT"]);
        let FieldValue::Bytes(bytes) = &record.fields[1].value else {
            panic!("expected EFIT bytes");
        };
        assert_eq!(bytes[0], 2, "paired EFIT kept");
    }

    #[test]
    fn master_resolved_fallback_filters_wrong_type_keyword() {
        // A FURN/NPC_ KWDA keyword that resolves to a MASTER STAT/SNDR (absent
        // from the output `fk_to_sig` map) must still be filtered out via the
        // per-record master fallback.
        let interner = StringInterner::new();
        let master_stat = make_fk(0x0010ABCD, "Fallout4.esm", &interner);
        let good = make_fk(0x0037D0B2, "Out.esp", &interner); // output KYWD
        let mut record = make_record(
            "FURN",
            vec![kw_list_field("KWDA", &[good, master_stat])],
            &interner,
        );
        let output_map = sig_map(&[(0x0037D0B2, "Out.esp", "KYWD")], &interner);
        let master_map = sig_map(&[(0x0010ABCD, "Fallout4.esm", "STAT")], &interner);

        let removed = filter_keyword_list_entries(
            &mut record,
            SubrecordSig::from_str("KWDA").unwrap(),
            &allows_only(&["KYWD"]),
            &output_map,
            &master_map,
        );

        assert_eq!(
            removed, 1,
            "the master STAT keyword is filtered via the fallback"
        );
        let FieldValue::List(items) = &record.fields[0].value else {
            panic!("expected List");
        };
        assert_eq!(items.len(), 1, "the valid KYWD survives");
    }

    fn union_bytes(sub: &str, raw: &[u8]) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sub).unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::from_slice(raw)),
        }
    }

    #[test]
    fn union_fk_offset_discriminates_sndr_bnam_variants() {
        // 4-byte base_descriptor variant → FK at offset 0; 6-byte values scalar
        // variant → no FK (the benign floor).
        assert_eq!(union_fk_offset("BNAM", &[0x11, 0x22, 0x33, 0x07]), Some(0));
        assert_eq!(
            union_fk_offset("BNAM", &[0, 1, 2, 3, 4, 5]),
            None,
            "6-byte values scalar is not a FK"
        );
    }

    #[test]
    fn strips_sndr_bnam_base_descriptor_when_wrong_type() {
        // Bucket 8: SNDR.BNAM base_descriptor (4-byte formid) resolves to a CELL
        // (wrong type, expected SNDR) → strip the whole BNAM (absent = non-
        // AutoWeapon, valid).
        let interner = StringInterner::new();
        let raw = 0x07ABCDEF_u32.to_le_bytes();
        let mut record = make_record("SNDR", vec![union_bytes("BNAM", &raw)], &interner);
        let mut resolved = FxHashMap::default();
        resolved.insert(0x07ABCDEF, SigCode::from_str("CELL").unwrap());

        let acted = validate_union_formid_target(
            &mut record,
            "BNAM",
            SubrecordSig::from_str("BNAM").unwrap(),
            UnionTypeRule::AllowOnly(&["SNDR"]),
            UnionAction::StripSubrecord,
            &resolved,
        );

        assert_eq!(acted, 1);
        assert!(
            record.fields.is_empty(),
            "wrong-type BNAM base_descriptor stripped"
        );
    }

    #[test]
    fn keeps_sndr_bnam_base_descriptor_when_target_is_sndr() {
        let interner = StringInterner::new();
        let raw = 0x07ABCDEF_u32.to_le_bytes();
        let mut record = make_record("SNDR", vec![union_bytes("BNAM", &raw)], &interner);
        let mut resolved = FxHashMap::default();
        resolved.insert(0x07ABCDEF, SigCode::from_str("SNDR").unwrap());

        let acted = validate_union_formid_target(
            &mut record,
            "BNAM",
            SubrecordSig::from_str("BNAM").unwrap(),
            UnionTypeRule::AllowOnly(&["SNDR"]),
            UnionAction::StripSubrecord,
            &resolved,
        );

        assert_eq!(acted, 0, "a valid SNDR base_descriptor is kept");
        assert_eq!(record.fields.len(), 1);
    }

    #[test]
    fn strips_sndr_bnam_base_descriptor_when_unresolved() {
        let interner = StringInterner::new();
        let raw = 0x004FD271_u32.to_le_bytes();
        let mut record = make_record("SNDR", vec![union_bytes("BNAM", &raw)], &interner);

        let acted = validate_union_formid_target(
            &mut record,
            "BNAM",
            SubrecordSig::from_str("BNAM").unwrap(),
            UnionTypeRule::AllowOnly(&["SNDR"]),
            UnionAction::StripSubrecord,
            &FxHashMap::default(),
        );

        assert_eq!(acted, 1);
        assert!(
            record.fields.is_empty(),
            "dangling BNAM descriptor stripped"
        );
    }

    #[test]
    fn leaves_sndr_bnam_values_scalar_variant_untouched() {
        // The 6-byte `values` scalar variant must never be read as a FK even if
        // its leading 4 bytes happen to collide with a resolved wrong-type id.
        let interner = StringInterner::new();
        let raw: [u8; 6] = [0xEF, 0xCD, 0xAB, 0x07, 0xB0, 0x04];
        let mut record = make_record("SNDR", vec![union_bytes("BNAM", &raw)], &interner);
        let mut resolved = FxHashMap::default();
        resolved.insert(0x07ABCDEF, SigCode::from_str("CELL").unwrap());

        let acted = validate_union_formid_target(
            &mut record,
            "BNAM",
            SubrecordSig::from_str("BNAM").unwrap(),
            UnionTypeRule::AllowOnly(&["SNDR"]),
            UnionAction::StripSubrecord,
            &resolved,
        );

        assert_eq!(acted, 0, "6-byte values scalar is never treated as a FK");
        assert_eq!(record.fields.len(), 1);
    }

    #[test]
    fn nulls_pack_ptda_fk_when_target_is_structural_type() {
        // Bucket 4-wrongtype: a PACK PTDA kind-0 reference whose FK resolves to a
        // LAND (a remap collision) is rewritten to Self; the data-input block
        // stays intact.
        let interner = StringInterner::new();
        let mut raw = Vec::new();
        raw.extend_from_slice(&0i32.to_le_bytes()); // kind 0 = reference
        raw.extend_from_slice(&0x07112233u32.to_le_bytes()); // FK @4
        raw.extend_from_slice(&0u32.to_le_bytes());
        let mut record = make_record("PACK", vec![union_bytes("PTDA", &raw)], &interner);
        let mut resolved = FxHashMap::default();
        resolved.insert(0x07112233, SigCode::from_str("LAND").unwrap());

        let acted = validate_union_formid_target(
            &mut record,
            "PTDA",
            SubrecordSig::from_str("PTDA").unwrap(),
            UnionTypeRule::Deny(PACK_TARGET_DENY),
            UnionAction::NullFk,
            &resolved,
        );

        assert_eq!(acted, 1);
        // Subrecord kept, target selector made self-relative.
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected bytes");
        };
        let kind = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let fk = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        assert_eq!(kind, PACK_TARGET_SELF_TYPE);
        assert_eq!(fk, 0, "wrong-type PTDA target nulled");
        assert_eq!(bytes.len(), 12, "data-input block kept intact");
    }

    #[test]
    fn pack_ptda_lvli_target_becomes_self() {
        let interner = StringInterner::new();
        let mut raw = Vec::new();
        raw.extend_from_slice(&1i32.to_le_bytes()); // kind 1 = object_id
        raw.extend_from_slice(&0x07556677u32.to_le_bytes());
        raw.extend_from_slice(&0u32.to_le_bytes());
        let mut record = make_record("PACK", vec![union_bytes("PTDA", &raw)], &interner);
        let mut resolved = FxHashMap::default();
        resolved.insert(0x07556677, SigCode::from_str("LVLI").unwrap());

        let acted = validate_union_formid_target(
            &mut record,
            "PTDA",
            SubrecordSig::from_str("PTDA").unwrap(),
            UnionTypeRule::Deny(PACK_TARGET_DENY),
            UnionAction::NullFk,
            &resolved,
        );

        assert_eq!(acted, 1);
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected bytes");
        };
        assert_eq!(
            i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            PACK_TARGET_SELF_TYPE
        );
        assert_eq!(
            u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            0
        );
    }

    #[test]
    fn pack_ptda_nonpersistent_ref_becomes_self() {
        let interner = StringInterner::new();
        let mut raw = Vec::new();
        raw.extend_from_slice(&0i32.to_le_bytes()); // kind 0 = reference
        raw.extend_from_slice(&0x00063D4Au32.to_le_bytes());
        raw.extend_from_slice(&0u32.to_le_bytes());
        let mut record = make_record("PACK", vec![union_bytes("PTDA", &raw)], &interner);
        let invalid = FxHashSet::from_iter([0x00063D4A]);

        let acted = benignify_pack_ptda_refs(&mut record, &invalid);

        assert_eq!(acted, 1);
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected bytes");
        };
        assert_eq!(
            i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            PACK_TARGET_SELF_TYPE
        );
        assert_eq!(
            u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            0
        );
    }

    #[test]
    fn keeps_pack_ptda_fk_when_target_is_valid_placed_ref() {
        let interner = StringInterner::new();
        let mut raw = Vec::new();
        raw.extend_from_slice(&0i32.to_le_bytes()); // kind 0 = reference
        raw.extend_from_slice(&0x07445566u32.to_le_bytes());
        raw.extend_from_slice(&0u32.to_le_bytes());
        let mut record = make_record("PACK", vec![union_bytes("PTDA", &raw)], &interner);
        let mut resolved = FxHashMap::default();
        resolved.insert(0x07445566, SigCode::from_str("REFR").unwrap());

        let acted = validate_union_formid_target(
            &mut record,
            "PTDA",
            SubrecordSig::from_str("PTDA").unwrap(),
            UnionTypeRule::Deny(PACK_TARGET_DENY),
            UnionAction::NullFk,
            &resolved,
        );

        assert_eq!(acted, 0, "a REFR placed target is valid — kept");
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected bytes");
        };
        let fk = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        assert_eq!(fk, 0x07445566);
    }

    #[test]
    fn leaves_pack_ptda_scalar_kind_untouched() {
        // kind 2 (object_type, a u32 form-type code) is a scalar — never read as
        // a FK, even when its offset-4 word resolves to a denied type.
        let interner = StringInterner::new();
        let mut raw = Vec::new();
        raw.extend_from_slice(&2i32.to_le_bytes()); // kind 2 = object_type scalar
        raw.extend_from_slice(&0x0000000Fu32.to_le_bytes());
        raw.extend_from_slice(&0u32.to_le_bytes());
        let mut record = make_record("PACK", vec![union_bytes("PTDA", &raw)], &interner);
        let mut resolved = FxHashMap::default();
        resolved.insert(0x0000000F, SigCode::from_str("LAND").unwrap());

        let acted = validate_union_formid_target(
            &mut record,
            "PTDA",
            SubrecordSig::from_str("PTDA").unwrap(),
            UnionTypeRule::Deny(PACK_TARGET_DENY),
            UnionAction::NullFk,
            &resolved,
        );

        assert_eq!(acted, 0, "kind-2 object_type scalar is not a FK");
    }

    #[test]
    fn collect_union_fk_raws_reads_only_fk_variants() {
        let interner = StringInterner::new();
        let bnam_fk = 0x07ABCDEF_u32.to_le_bytes();
        let bnam_scalar: [u8; 6] = [1, 2, 3, 4, 5, 6];
        let record = make_record(
            "SNDR",
            vec![
                union_bytes("BNAM", &bnam_fk),
                union_bytes("BNAM", &bnam_scalar),
            ],
            &interner,
        );
        let raws = collect_union_fk_raws(&record, "BNAM", SubrecordSig::from_str("BNAM").unwrap());
        assert_eq!(
            raws,
            vec![0x07ABCDEF],
            "only the 4-byte FK variant is collected"
        );
    }

    fn sndd_rows(rows: &[(u32, u32)]) -> FieldEntry {
        let mut raw = Vec::new();
        for (ty, fk) in rows {
            raw.extend_from_slice(&ty.to_le_bytes());
            raw.extend_from_slice(&fk.to_le_bytes());
        }
        FieldEntry {
            sig: SubrecordSig::from_str("SNDD").unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::from_vec(raw)),
        }
    }

    fn rdsa_rows(rows: &[(u32, u32, u32)]) -> FieldEntry {
        let mut raw = Vec::new();
        for (fk, chance, flags) in rows {
            raw.extend_from_slice(&fk.to_le_bytes());
            raw.extend_from_slice(&chance.to_le_bytes());
            raw.extend_from_slice(&flags.to_le_bytes());
        }
        FieldEntry {
            sig: SubrecordSig::from_str("RDSA").unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::from_vec(raw)),
        }
    }

    fn dobj_rows(rows: &[(u32, u32)]) -> FieldEntry {
        let mut raw = Vec::new();
        for (tag, object_id) in rows {
            raw.extend_from_slice(&tag.to_le_bytes());
            raw.extend_from_slice(&object_id.to_le_bytes());
        }
        FieldEntry {
            sig: SubrecordSig::from_str("DNAM").unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::from_vec(raw)),
        }
    }

    #[test]
    fn drops_mgef_sndd_row_when_sound_is_wrong_type() {
        // Bucket 12: MGEF.SNDD sound row 0 resolves to a master REFR (wrong type,
        // expected SNDR) → drop that row; a valid SNDR row is kept.
        let interner = StringInterner::new();
        let mut record = make_record(
            "MGEF",
            vec![sndd_rows(&[(0, 0x0010FBAB), (1, 0x07445566)])],
            &interner,
        );
        let mut resolved = FxHashMap::default();
        resolved.insert(0x0010FBAB, SigCode::from_str("REFR").unwrap()); // master REFR
        resolved.insert(0x07445566, SigCode::from_str("SNDR").unwrap()); // valid

        let dropped = drop_array_rows_with_null_or_wrong_type_fks(
            &mut record,
            SubrecordSig::from_str("SNDD").unwrap(),
            8,
            4,
            &["SNDR"],
            &resolved,
        );

        assert_eq!(dropped, 1);
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected bytes");
        };
        assert_eq!(bytes.len(), 8, "wrong-type row removed");
        let row0_fk = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        assert_eq!(row0_fk, 0x07445566, "valid SNDR sound kept");
    }

    #[test]
    fn drops_mgef_sndd_null_row_and_removes_empty_subrecord() {
        let interner = StringInterner::new();
        let mut record = make_record("MGEF", vec![sndd_rows(&[(0, 0)])], &interner);

        let dropped = drop_array_rows_with_null_or_wrong_type_fks(
            &mut record,
            SubrecordSig::from_str("SNDD").unwrap(),
            8,
            4,
            &["SNDR"],
            &FxHashMap::default(),
        );

        assert_eq!(dropped, 1);
        assert!(
            record.fields.is_empty(),
            "SNDD subrecord removed when every row is invalid"
        );
    }

    #[test]
    fn nulls_regn_rdsa_row_when_sound_is_wrong_type() {
        let interner = StringInterner::new();
        let mut record = make_record(
            "REGN",
            vec![rdsa_rows(&[
                (0x0010FBAB, 50, 0),
                (0x07445566, 75, 0),
                (0x07556677, 100, 0),
            ])],
            &interner,
        );
        let mut resolved = FxHashMap::default();
        resolved.insert(0x0010FBAB, SigCode::from_str("REFR").unwrap());
        resolved.insert(0x07445566, SigCode::from_str("SNDR").unwrap());
        resolved.insert(0x07556677, SigCode::from_str("SOUN").unwrap());

        let nulled = null_array_row_wrong_type_fks(
            &mut record,
            SubrecordSig::from_str("RDSA").unwrap(),
            12,
            0,
            &["SNDR", "SOUN"],
            &resolved,
        );

        assert_eq!(nulled, 1);
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected bytes");
        };
        let row0_fk = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let row1_fk = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
        let row2_fk = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
        assert_eq!(row0_fk, 0, "wrong-type regional sound nulled");
        assert_eq!(row1_fk, 0x07445566, "valid SNDR regional sound kept");
        assert_eq!(row2_fk, 0x07556677, "valid SOUN regional sound kept");
    }

    #[test]
    fn collect_array_row_fk_raws_skips_null_and_reads_each_row() {
        let interner = StringInterner::new();
        let record = make_record(
            "MGEF",
            vec![sndd_rows(&[(0, 0x0010FBAB), (1, 0), (2, 0x07445566)])],
            &interner,
        );
        let raws =
            collect_array_row_fk_raws(&record, SubrecordSig::from_str("SNDD").unwrap(), 8, 4);
        assert_eq!(raws, vec![0x0010FBAB, 0x07445566], "null row skipped");
    }

    #[test]
    fn drops_fo76_only_dobj_object_use_raw_rows() {
        let interner = StringInterner::new();
        let mut record = make_record(
            "DOBJ",
            vec![dobj_rows(&[
                (0x3346_4141, 0x07001111), // AAF3
                (0x444C_4F47, 0x07002222), // GOLD-style keeper
            ])],
            &interner,
        );

        let dropped = drop_fo76_only_dobj_object_use_rows(&mut record, &interner);

        assert_eq!(dropped, 1);
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected bytes");
        };
        assert_eq!(bytes.len(), 8);
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x444C_4F47
        );
    }

    #[test]
    fn drops_fo76_only_dobj_object_use_struct_rows() {
        let interner = StringInterner::new();
        let tag = interner.intern("Tag");
        let object = interner.intern("Object");
        let mut record = make_record(
            "DOBJ",
            vec![FieldEntry {
                sig: SubrecordSig::from_str("DNAM").unwrap(),
                value: FieldValue::List(vec![
                    FieldValue::Struct(vec![
                        (tag, FieldValue::Uint(0x5350_474C)), // LGPS
                        (object, FieldValue::Uint(0x07001111)),
                    ]),
                    FieldValue::Struct(vec![
                        (tag, FieldValue::Uint(0x544C_4153)), // SALT keeper
                        (object, FieldValue::Uint(0x07002222)),
                    ]),
                ]),
            }],
            &interner,
        );

        let dropped = drop_fo76_only_dobj_object_use_rows(&mut record, &interner);

        assert_eq!(dropped, 1);
        let FieldValue::List(items) = &record.fields[0].value else {
            panic!("expected rows");
        };
        assert_eq!(items.len(), 1);
        assert_eq!(dobj_object_use_tag(&items[0], &interner), Some(0x544C_4153));
    }

    // ── REFR placed-child Class C (post-copy strip) ─────────────────────────

    #[test]
    fn strips_wrong_type_xlyr_pointing_at_output_collision() {
        // XLYR (→LAYR) remapped onto an unrelated output REFR/LVLI (xEdit "Found a
        // REFR/LVLI reference, expected: LAYR"). LAYR is never carried into the
        // exterior-only port → strip the wrong-type, like XRFG.
        let interner = StringInterner::new();
        let xlyr_fk = make_fk(0x07A12345, "Out.esp", &interner);
        let mut record = make_record("REFR", vec![fk_field("XLYR", xlyr_fk)], &interner);
        let map = sig_map(&[(0x07A12345, "Out.esp", "LVLI")], &interner);

        let outcome = apply_to_record(
            &mut record,
            SubrecordSig::from_str("XLYR").unwrap(),
            Action::Strip,
            true, // XLYR is in STRIP_DANGLING_SUBRECORDS
            &allows_only(&["LAYR"]),
            &map,
            &FxHashMap::default(),
        );

        assert!(outcome.changed);
        assert_eq!(outcome.acted, 1);
        assert!(record.fields.is_empty(), "wrong-type XLYR stripped");
    }

    #[test]
    fn strips_wrong_type_xasp_pointing_at_land() {
        // XASP (→REFR) remapped onto a LAND (xEdit "Found a LAND reference,
        // expected: REFR"). REFR IS a valid FO4 XASP target, but the FO76 acoustic-
        // parent REFR lived in a dropped interior, so the remap collided — strip
        // the positively-wrong-type slot.
        let interner = StringInterner::new();
        let xasp_fk = make_fk(0x07B22222, "Out.esp", &interner);
        let mut record = make_record("REFR", vec![fk_field("XASP", xasp_fk)], &interner);
        let map = sig_map(&[(0x07B22222, "Out.esp", "LAND")], &interner);

        let outcome = apply_to_record(
            &mut record,
            SubrecordSig::from_str("XASP").unwrap(),
            Action::Strip,
            false, // XASP is NOT in STRIP_DANGLING_SUBRECORDS (REFR is a real target)
            &allows_only(&["REFR"]),
            &map,
            &FxHashMap::default(),
        );

        assert!(outcome.changed);
        assert_eq!(outcome.acted, 1);
        assert!(record.fields.is_empty(), "wrong-type XASP stripped");
    }

    #[test]
    fn keeps_dangling_xasp_since_refr_is_a_real_fo4_target() {
        // A dangling (non-null, unresolved) XASP can still legitimately become a
        // REFR post-copy, so unlike XRFG/XLYR it is NOT strip-dangling — leave it
        // for the sweep fixups. Guards against over-stripping valid acoustic refs.
        let interner = StringInterner::new();
        let xasp_fk = make_fk(0x07C33333, "Out.esp", &interner);
        let mut record = make_record("REFR", vec![fk_field("XASP", xasp_fk)], &interner);
        let map = sig_map(&[], &interner); // resolves nowhere

        let outcome = apply_to_record(
            &mut record,
            SubrecordSig::from_str("XASP").unwrap(),
            Action::Strip,
            false,
            &allows_only(&["REFR"]),
            &map,
            &FxHashMap::default(),
        );

        assert!(!outcome.changed, "dangling XASP left for the sweep fixups");
        assert_eq!(record.fields.len(), 1);
    }

    #[test]
    fn refr_placed_child_table_is_subset_of_class_c_targets() {
        // The post-copy REFR strip must use the SAME entries as the in-phase
        // CLASS_C_TARGETS so both strips share the schema-derived action/allow
        // logic and never drift.
        for entry in REFR_PLACED_CHILD_CLASS_C {
            assert!(
                CLASS_C_TARGETS.contains(entry),
                "{entry:?} missing from CLASS_C_TARGETS — post-copy strip would diverge"
            );
            assert_eq!(entry.0, "REFR", "post-copy table is REFR-only");
        }
    }

    // -----------------------------------------------------------------------
    // SCOL ONAM+DATA paired-part strip
    // -----------------------------------------------------------------------

    /// Build a minimal SCOL record with the given alternating ONAM/DATA fields.
    fn make_scol(parts: &[(FormKey, bool)], interner: &StringInterner) -> Record {
        // `parts`: (onam_fk, include_data) — include_data=true adds a dummy DATA after ONAM.
        let mut fields: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
        for (fk, has_data) in parts {
            fields.push(FieldEntry {
                sig: SubrecordSig::from_str("ONAM").unwrap(),
                value: FieldValue::FormKey(*fk),
            });
            if *has_data {
                // Dummy 28-byte DATA (7 floats × 4 bytes).
                fields.push(FieldEntry {
                    sig: SubrecordSig::from_str("DATA").unwrap(),
                    value: FieldValue::Bytes(smallvec::smallvec![0u8; 28]),
                });
            }
        }
        make_record("SCOL", fields.into_vec(), interner)
    }

    #[test]
    fn scol_onam_data_pair_dropped_when_onam_resolves_to_scol() {
        // The one xEdit "Found a SCOL reference" case: v96_MainframeConsole03
        // ONAM 2 → v96_ArchiveConsole01 [SCOL]. The ONAM+DATA pair must be dropped
        // together (lockstep); remaining pairs are untouched.
        let interner = StringInterner::new();
        let stat_fk = make_fk(0x001111, "Out.esp", &interner); // → STAT: legal
        let scol_fk = make_fk(0x46BC66, "Out.esp", &interner); // → SCOL: wrong type
        // 3 parts: STAT-pair, SCOL-pair, STAT-pair. Middle pair must be stripped.
        let mut record = make_scol(
            &[(stat_fk, true), (scol_fk, true), (stat_fk, true)],
            &interner,
        );
        let map = sig_map(
            &[(0x001111, "Out.esp", "STAT"), (0x46BC66, "Out.esp", "SCOL")],
            &interner,
        );

        let acted = strip_scol_wrong_type_onam_data_pairs(&mut record, &map, &FxHashMap::default());

        assert_eq!(acted, 1, "one ONAM+DATA pair dropped");
        // 2 remaining parts → 4 subrecords (ONAM, DATA, ONAM, DATA).
        assert_eq!(record.fields.len(), 4, "two valid pairs remain");
        // Verify no ONAM left pointing at the SCOL fk.
        for entry in &record.fields {
            if entry.sig.as_str() == "ONAM" {
                let FieldValue::FormKey(fk) = &entry.value else {
                    panic!("ONAM must decode to FormKey");
                };
                assert_ne!(
                    fk.local, scol_fk.local,
                    "the SCOL-targeting ONAM must have been removed"
                );
            }
        }
    }

    #[test]
    fn scol_onam_data_pair_kept_when_onam_resolves_to_stat() {
        // A SCOL part whose ONAM resolves to a STAT (the overwhelmingly common
        // case) must be left intact — strip only SCOL-typed targets.
        let interner = StringInterner::new();
        let stat_fk = make_fk(0x001111, "Out.esp", &interner);
        let mut record = make_scol(&[(stat_fk, true)], &interner);
        let map = sig_map(&[(0x001111, "Out.esp", "STAT")], &interner);

        let acted = strip_scol_wrong_type_onam_data_pairs(&mut record, &map, &FxHashMap::default());

        assert_eq!(acted, 0, "no pairs dropped — STAT target is legal");
        assert_eq!(record.fields.len(), 2, "ONAM+DATA unchanged");
    }

    #[test]
    fn scol_onam_data_pair_dropped_when_onam_is_null() {
        let interner = StringInterner::new();
        let null_fk = make_fk(0, "Out.esp", &interner);
        let stat_fk = make_fk(0x001111, "Out.esp", &interner);
        let mut record = make_scol(&[(null_fk, true), (stat_fk, true)], &interner);
        let map = sig_map(&[(0x001111, "Out.esp", "STAT")], &interner);

        let acted = strip_scol_wrong_type_onam_data_pairs(&mut record, &map, &FxHashMap::default());

        assert_eq!(acted, 1, "null ONAM+DATA pair dropped");
        assert_eq!(record.fields.len(), 2, "valid pair remains");
        let FieldValue::FormKey(fk) = &record.fields[0].value else {
            panic!("expected ONAM FormKey");
        };
        assert_eq!(fk.local, stat_fk.local);
    }

    #[test]
    fn scol_mnam_subrecord_is_stripped() {
        let interner = StringInterner::new();
        let stat_fk = make_fk(0x001111, "Out.esp", &interner);
        let mut record = make_scol(&[(stat_fk, true)], &interner);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MNAM").unwrap(),
            value: FieldValue::Bytes(smallvec::smallvec![0u8; 16]),
        });

        let acted = strip_scol_mnam_subrecords(&mut record);

        assert_eq!(acted, 1);
        assert!(
            record
                .fields
                .iter()
                .all(|entry| entry.sig.as_str() != "MNAM"),
            "SCOL MNAM stripped"
        );
        assert_eq!(record.fields.len(), 2, "ONAM+DATA pair kept");
    }

    #[test]
    fn scol_no_onam_subrecords_is_no_op() {
        // A zero-ONAM SCOL (Root Cause A: NIF-only, no parts) must pass through
        // unchanged — the function must not panic or miscount.
        let interner = StringInterner::new();
        let mut record = make_scol(&[], &interner);
        let map: FxHashMap<(u32, Sym), SigCode> = FxHashMap::default();

        let acted = strip_scol_wrong_type_onam_data_pairs(&mut record, &map, &FxHashMap::default());

        assert_eq!(acted, 0);
        assert!(record.fields.is_empty());
    }

    #[test]
    fn scol_dangling_onam_left_for_sweep_fixups() {
        // A non-null ONAM that doesn't resolve (neither output nor master) is
        // dangling — conservative policy keeps it for the sweep fixups.
        let interner = StringInterner::new();
        let dangling_fk = make_fk(0x00DEAD, "Out.esp", &interner);
        let mut record = make_scol(&[(dangling_fk, true)], &interner);
        let map: FxHashMap<(u32, Sym), SigCode> = FxHashMap::default(); // resolves nowhere

        let acted = strip_scol_wrong_type_onam_data_pairs(&mut record, &map, &FxHashMap::default());

        assert_eq!(acted, 0, "dangling ONAM left intact");
        assert_eq!(record.fields.len(), 2);
    }
}
