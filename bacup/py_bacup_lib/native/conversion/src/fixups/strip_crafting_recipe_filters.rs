//! Fixup: drop COBJ `FNAM` ("Category" recipe filters) from item- and
//! modification-crafting recipes.
//!
//! FO76 tabs every crafting menu by `RecipeFilter` keywords, so armour, apparel and
//! paint recipes carry an `FNAM` (`RecipeFilter_Armor_Backpack`,
//! `RecipeFilter_PowerArmor_Excavator`, ...) that translation carries verbatim. FO4
//! filters only the workshop build menu and the chem/cooking stations this way: none
//! of the 1542 COBJs with an `FNAM` across `Fallout4.esm` and all six DLCs creates an
//! `ARMO` or `OMOD`, since apparel and mod recipes are enumerated by attach point. A
//! filter files them under a tab FO4 never builds, so they never appear.
//!
//! `FNAM` is removed from COBJs whose workbench (`BNAM`) is an armour, weapon,
//! power-armour, or settlement-workshop bench, and from COBJs with no workbench
//! (mostly OMOD paint/mod recipes). FO76 recipe categories are unsupported in the
//! conversion; the retained `BNAM` keeps supported settlement objects exposed. Chem,
//! cooking, brewing, cannery and tinker's recipes keep their filters, since FO4 tabs
//! those benches the same way.
//!
//! Must run after `apply_fo76_workshop_catalog`, which stamps the workshop `BNAM`
//! read here. `FNAM` is a lone `formid_array` with no paired count subrecord, so
//! removing it needs no count fixup.

use rustc_hash::FxHashSet;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

/// `PowerArmorWorkbenchKeyword`, the FO4 bench converted paint recipes are
/// re-pointed at.
const FO4_POWER_ARMOR_WORKBENCH: u32 = 0x0016_FF00;
const FO4_MASTER_PLUGIN: &str = "Fallout4.esm";

const FO4_WORKSHOP_WORKBENCH_IDS: [u32; 6] = [
    0x0005_A0C8,
    0x0005_B5E3,
    0x0008_280B,
    0x0005_A0CA,
    0x0012_E2C8,
    0x0024_6F85,
];

/// FO76 workbench keywords whose recipes produce or modify a worn item.
const ITEM_CRAFTING_WORKBENCH_EDITOR_IDS: [&str; 2] =
    ["Workbench_Crafting_Armor", "Workbench_Crafting_Weapon"];

pub struct StripCraftingRecipeFiltersFixup;

impl Fixup for StripCraftingRecipeFiltersFixup {
    fn name(&self) -> &'static str {
        "strip_crafting_recipe_filters"
    }

    fn uses_session(&self) -> bool {
        true
    }

    /// Gated on the target, not the source: the rule encodes FO4 menu
    /// semantics. It is inert for the other pairs anyway — neither Skyrim's
    /// COBJ nor FNV/FO3's recipe records carry a category, and a record without
    /// `FNAM` is never touched.
    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        session.target_slot().parsed.game.as_deref() == Some("fo4")
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
        let cobj_sig =
            SigCode::from_str("COBJ").map_err(|e| FixupError::SchemaError(e.to_string()))?;

        let mut report = FixupReport::empty();
        let benches = collect_item_crafting_workbenches(session, mapper.interner, &mut report)?;

        let fks = session
            .form_keys_of_sig(cobj_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        let mut changed_records = Vec::new();
        for fk in fks {
            let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                Ok(r) => r,
                Err(e) => {
                    let w = mapper
                        .interner
                        .intern(&format!("strip_crafting_recipe_filters_read_err:{e}"));
                    report.warnings.push(w);
                    continue;
                }
            };
            if strip_recipe_filters(&mut record, &benches) {
                changed_records.push(record);
            }
        }

        let expected = changed_records.len();
        if expected == 0 {
            return Ok(report);
        }
        let replaced = session
            .replace_records_contents(changed_records, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "strip_crafting_recipe_filters replaced {replaced} of {expected} expected records"
            )));
        }
        report.records_changed = replaced.try_into().unwrap_or(u32::MAX);
        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Gather
// ---------------------------------------------------------------------------

/// FormKeys of every workbench keyword whose recipes must not carry a category:
/// the FO76 armour and weapon benches (matched by EditorID in the output
/// plugin) plus FO4's own power-armour bench.
///
/// `pub(crate)`: shared with the store2 sweep visitor so both drivers gather
/// through the identical code path.
pub(crate) fn collect_item_crafting_workbenches(
    session: &mut PluginSession,
    interner: &StringInterner,
    report: &mut FixupReport,
) -> Result<FxHashSet<FormKey>, FixupError> {
    let kywd_sig = SigCode::from_str("KYWD").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let mut out = FxHashSet::default();
    out.insert(FormKey {
        local: FO4_POWER_ARMOR_WORKBENCH,
        plugin: interner.intern(FO4_MASTER_PLUGIN),
    });
    out.extend(FO4_WORKSHOP_WORKBENCH_IDS.into_iter().map(|local| FormKey {
        local,
        plugin: interner.intern(FO4_MASTER_PLUGIN),
    }));

    let fks = session
        .form_keys_of_sig(kywd_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    for fk in fks {
        let Ok(Some(bytes)) = session.first_subrecord_bytes(&fk, "EDID") else {
            continue;
        };
        let eid = String::from_utf8_lossy(&bytes);
        let eid = eid.trim_end_matches('\0');
        if ITEM_CRAFTING_WORKBENCH_EDITOR_IDS
            .iter()
            .any(|name| name.eq_ignore_ascii_case(eid))
        {
            out.insert(fk);
        }
    }

    // Two FO76 benches plus the FO4 item and workshop benches. Fewer means a bench keyword was
    // renamed or dropped and its recipes will keep categories they should not.
    let expected_benches =
        ITEM_CRAFTING_WORKBENCH_EDITOR_IDS.len() + 1 + FO4_WORKSHOP_WORKBENCH_IDS.len();
    if out.len() < expected_benches {
        let w = interner.intern(&format!(
            "strip_crafting_recipe_filters:resolved {} of {} item-crafting workbench keywords",
            out.len(),
            expected_benches
        ));
        report.warnings.push(w);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Record-level mutation (extracted for unit-test and visitor access)
// ---------------------------------------------------------------------------

/// Remove every `FNAM` from `record` when its workbench is an item-crafting or
/// settlement-workshop bench, or it has no workbench at all.
///
/// Returns `true` when at least one `FNAM` was removed. Idempotent: a record
/// already free of `FNAM` is left untouched.
pub fn strip_recipe_filters(
    record: &mut Record,
    item_crafting_benches: &FxHashSet<FormKey>,
) -> bool {
    let fnam_sig = SubrecordSig(*b"FNAM");
    if !record.fields.iter().any(|entry| entry.sig == fnam_sig) {
        return false;
    }
    if !is_item_crafting_recipe(record, item_crafting_benches) {
        return false;
    }
    record.fields.retain(|entry| entry.sig != fnam_sig);
    true
}

fn is_item_crafting_recipe(record: &Record, item_crafting_benches: &FxHashSet<FormKey>) -> bool {
    match workbench_of(record) {
        Some(bench) => item_crafting_benches.contains(&bench),
        None => true,
    }
}

/// The recipe's workbench keyword, or `None` when `BNAM` is absent or NULL.
fn workbench_of(record: &Record) -> Option<FormKey> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig.0 == *b"BNAM")
        .and_then(|entry| first_form_key(&entry.value))
        .filter(|fk| fk.local != 0)
}

fn first_form_key(value: &FieldValue) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(fk) => Some(*fk),
        FieldValue::List(values) => values.iter().find_map(first_form_key),
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, value)| first_form_key(value)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SigCode;
    use crate::record::{FieldEntry, RecordFlags};
    use crate::sym::StringInterner;
    use smallvec::SmallVec;

    fn fk(local: u32, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    fn field(sig: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        }
    }

    fn cobj(fields: Vec<FieldEntry>, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str("COBJ").unwrap(),
            form_key: fk(0x0800, "Output.esp", interner),
            eid: Some(interner.intern("ATX_co_Recipe")),
            flags: RecordFlags::empty(),
            fields: fields.into_iter().collect(),
            warnings: SmallVec::new(),
        }
    }

    fn fnam(local: u32, interner: &StringInterner) -> FieldEntry {
        field(
            "FNAM",
            FieldValue::List(vec![FieldValue::FormKey(fk(local, "Output.esp", interner))]),
        )
    }

    fn bnam(local: u32, plugin: &str, interner: &StringInterner) -> FieldEntry {
        field("BNAM", FieldValue::FormKey(fk(local, plugin, interner)))
    }

    /// The armour bench + the weapon bench + FO4's power-armour bench.
    fn benches(interner: &StringInterner) -> FxHashSet<FormKey> {
        let mut out: FxHashSet<FormKey> = [
            fk(0x1F6062, "Output.esp", interner),
            fk(0x1F6063, "Output.esp", interner),
            fk(FO4_POWER_ARMOR_WORKBENCH, FO4_MASTER_PLUGIN, interner),
        ]
        .into_iter()
        .collect();
        out.extend(
            FO4_WORKSHOP_WORKBENCH_IDS
                .into_iter()
                .map(|local| fk(local, FO4_MASTER_PLUGIN, interner)),
        );
        out
    }

    // -----------------------------------------------------------------------

    #[test]
    fn strips_filter_from_armor_bench_recipe() {
        let interner = StringInterner::new();
        // ATX_co_Armor_Backpack_HeirloomBasket: armour bench + BACKPACK filter.
        let mut record = cobj(
            vec![
                bnam(0x1F6062, "Output.esp", &interner),
                fnam(0x47EF39, &interner),
            ],
            &interner,
        );

        assert!(strip_recipe_filters(&mut record, &benches(&interner)));
        assert!(!record.fields.iter().any(|e| e.sig.0 == *b"FNAM"));
        assert!(
            record.fields.iter().any(|e| e.sig.0 == *b"BNAM"),
            "the workbench link must survive"
        );
    }

    #[test]
    fn strips_filter_from_power_armor_bench_recipe() {
        let interner = StringInterner::new();
        let mut record = cobj(
            vec![
                bnam(FO4_POWER_ARMOR_WORKBENCH, FO4_MASTER_PLUGIN, &interner),
                fnam(0x4EAEF1, &interner),
            ],
            &interner,
        );

        assert!(strip_recipe_filters(&mut record, &benches(&interner)));
        assert!(!record.fields.iter().any(|e| e.sig.0 == *b"FNAM"));
    }

    #[test]
    fn strips_filter_when_recipe_has_no_workbench() {
        let interner = StringInterner::new();
        // ATX_co_mod_PowerArmor_Excavator_*: OMOD recipe, no BNAM at all.
        let mut record = cobj(
            vec![
                field(
                    "CNAM",
                    FieldValue::FormKey(fk(0x537067, "Output.esp", &interner)),
                ),
                fnam(0x4EAEF1, &interner),
            ],
            &interner,
        );

        assert!(strip_recipe_filters(&mut record, &benches(&interner)));
        assert!(!record.fields.iter().any(|e| e.sig.0 == *b"FNAM"));
        assert!(record.fields.iter().any(|e| e.sig.0 == *b"CNAM"));
    }

    #[test]
    fn strips_filter_when_workbench_link_is_null() {
        let interner = StringInterner::new();
        let mut record = cobj(
            vec![bnam(0, "Output.esp", &interner), fnam(0x47EF39, &interner)],
            &interner,
        );

        assert!(strip_recipe_filters(&mut record, &benches(&interner)));
        assert!(!record.fields.iter().any(|e| e.sig.0 == *b"FNAM"));
    }

    #[test]
    fn strips_filter_from_workshop_recipe() {
        let interner = StringInterner::new();
        // apply_fo76_workshop_catalog re-pointed this at
        // WorkshopWorkbenchTypeDecorations; its category is the FO4 menu node.
        let mut record = cobj(
            vec![
                bnam(0x08280B, FO4_MASTER_PLUGIN, &interner),
                fnam(0x0249E89, &interner),
            ],
            &interner,
        );

        assert!(strip_recipe_filters(&mut record, &benches(&interner)));
        assert!(!record.fields.iter().any(|e| e.sig.0 == *b"FNAM"));
    }

    #[test]
    fn keeps_filter_on_cooking_recipe() {
        let interner = StringInterner::new();
        // Workbench_Crafting_Cooking is not an item-crafting bench: FO4 tabs
        // its chem/cooking counterpart the same way.
        let mut record = cobj(
            vec![
                bnam(0x1F6070, "Output.esp", &interner),
                fnam(0x102150, &interner),
            ],
            &interner,
        );

        assert!(!strip_recipe_filters(&mut record, &benches(&interner)));
        assert_eq!(
            record.fields.iter().filter(|e| e.sig.0 == *b"FNAM").count(),
            1
        );
    }

    #[test]
    fn no_op_when_recipe_has_no_filter() {
        let interner = StringInterner::new();
        let mut record = cobj(vec![bnam(0x1F6062, "Output.esp", &interner)], &interner);

        assert!(!strip_recipe_filters(&mut record, &benches(&interner)));
        assert_eq!(record.fields.len(), 1);
    }

    #[test]
    fn strips_every_filter_entry_and_is_idempotent() {
        let interner = StringInterner::new();
        let mut record = cobj(
            vec![
                bnam(0x1F6062, "Output.esp", &interner),
                fnam(0x47EF39, &interner),
                fnam(0x471955, &interner),
            ],
            &interner,
        );

        assert!(strip_recipe_filters(&mut record, &benches(&interner)));
        assert!(!record.fields.iter().any(|e| e.sig.0 == *b"FNAM"));
        assert!(
            !strip_recipe_filters(&mut record, &benches(&interner)),
            "second pass must be a no-op"
        );
    }
}
