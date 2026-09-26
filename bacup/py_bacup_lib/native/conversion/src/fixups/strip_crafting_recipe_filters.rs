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
//! Weapon and armour recipes (`CNAM` a `WEAP` or `ARMO`) at the FO76 weapon and armour
//! benches also keep their filters. Tales' Craft mode lists them there in the tabbed
//! recipe menu, which drops a recipe without a category. FO4's own modify menu at these
//! benches never lists item recipes, so the filter changes nothing without Tales.
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

const GEAR_SIGS: [&str; 2] = ["WEAP", "ARMO"];

/// What decides whether a recipe loses its category.
pub struct RecipeFilterIndex {
    /// Workbench keywords whose recipes lose their category.
    pub(crate) benches: FxHashSet<FormKey>,
    /// The FO76 armour and weapon bench keywords.
    pub(crate) gear_benches: FxHashSet<FormKey>,
    /// Every `WEAP` and `ARMO` in the output plugin and its masters.
    pub(crate) gear: FxHashSet<FormKey>,
}

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
        let index = collect_recipe_filter_index(session, config, mapper.interner, &mut report)?;

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
            if strip_recipe_filters(&mut record, &index) {
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

/// The workbench keywords whose recipes must not carry a category: the FO76
/// armour and weapon benches (matched by EditorID in the output plugin), FO4's
/// own power-armour bench and the workshop benches. Also the weapons and armour
/// whose recipes at the FO76 benches keep theirs.
///
/// `pub(crate)`: shared with the store2 sweep visitor so both drivers gather
/// through the identical code path.
pub(crate) fn collect_recipe_filter_index(
    session: &mut PluginSession,
    config: &FixupConfig,
    interner: &StringInterner,
    report: &mut FixupReport,
) -> Result<RecipeFilterIndex, FixupError> {
    let kywd_sig = SigCode::from_str("KYWD").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let mut gear_benches = FxHashSet::default();
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
            gear_benches.insert(fk);
        }
    }

    let mut benches = gear_benches.clone();
    benches.insert(FormKey {
        local: FO4_POWER_ARMOR_WORKBENCH,
        plugin: interner.intern(FO4_MASTER_PLUGIN),
    });
    benches.extend(FO4_WORKSHOP_WORKBENCH_IDS.into_iter().map(|local| FormKey {
        local,
        plugin: interner.intern(FO4_MASTER_PLUGIN),
    }));

    // Two FO76 benches plus the FO4 item and workshop benches. Fewer means a bench keyword was
    // renamed or dropped and its recipes will keep categories they should not.
    let expected_benches =
        ITEM_CRAFTING_WORKBENCH_EDITOR_IDS.len() + 1 + FO4_WORKSHOP_WORKBENCH_IDS.len();
    if benches.len() < expected_benches {
        let w = interner.intern(&format!(
            "strip_crafting_recipe_filters:resolved {} of {} item-crafting workbench keywords",
            benches.len(),
            expected_benches
        ));
        report.warnings.push(w);
    }

    // Some FO76 recipes create a weapon or armour kept from FO4 or a DLC.
    let mut gear = FxHashSet::default();
    for sig in GEAR_SIGS {
        let sig = SigCode::from_str(sig).map_err(|e| FixupError::SchemaError(e.to_string()))?;
        gear.extend(
            session
                .form_keys_of_sig(sig, interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?,
        );
        for &handle_id in &config.target_master_handle_ids {
            if let Ok(fks) = session.form_keys_of_sig_in_handle(handle_id, sig, interner) {
                gear.extend(fks);
            }
        }
    }
    Ok(RecipeFilterIndex {
        benches,
        gear_benches,
        gear,
    })
}

// ---------------------------------------------------------------------------
// Record-level mutation (extracted for unit-test and visitor access)
// ---------------------------------------------------------------------------

/// Remove every `FNAM` from `record` when its workbench is an item-crafting or
/// settlement-workshop bench, or it has no workbench at all, unless it is a
/// weapon or armour recipe at an FO76 weapon or armour bench.
///
/// Returns `true` when at least one `FNAM` was removed. Idempotent: a record
/// already free of `FNAM` is left untouched.
pub fn strip_recipe_filters(record: &mut Record, index: &RecipeFilterIndex) -> bool {
    let fnam_sig = SubrecordSig(*b"FNAM");
    if !record.fields.iter().any(|entry| entry.sig == fnam_sig) {
        return false;
    }
    if !is_item_crafting_recipe(record, &index.benches) || crafts_gear(record, index) {
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

fn crafts_gear(record: &Record, index: &RecipeFilterIndex) -> bool {
    workbench_of(record).is_some_and(|bench| index.gear_benches.contains(&bench))
        && form_key_of(record, b"CNAM").is_some_and(|created| index.gear.contains(&created))
}

/// The recipe's workbench keyword, or `None` when `BNAM` is absent or NULL.
fn workbench_of(record: &Record) -> Option<FormKey> {
    form_key_of(record, b"BNAM")
}

fn form_key_of(record: &Record, sig: &[u8; 4]) -> Option<FormKey> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig.0 == *sig)
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

    const ARMOR: u32 = 0x0900;
    const WEAPON: u32 = 0x004822;

    /// The armour bench + the weapon bench + FO4's power-armour and workshop
    /// benches, an FO76 armour and an FO4 weapon.
    fn index(interner: &StringInterner) -> RecipeFilterIndex {
        let gear_benches: FxHashSet<FormKey> = [
            fk(0x1F6062, "Output.esp", interner),
            fk(0x1F6063, "Output.esp", interner),
        ]
        .into_iter()
        .collect();
        let mut benches = gear_benches.clone();
        benches.insert(fk(FO4_POWER_ARMOR_WORKBENCH, FO4_MASTER_PLUGIN, interner));
        benches.extend(
            FO4_WORKSHOP_WORKBENCH_IDS
                .into_iter()
                .map(|local| fk(local, FO4_MASTER_PLUGIN, interner)),
        );
        let gear = [
            fk(ARMOR, "Output.esp", interner),
            fk(WEAPON, FO4_MASTER_PLUGIN, interner),
        ]
        .into_iter()
        .collect();
        RecipeFilterIndex {
            benches,
            gear_benches,
            gear,
        }
    }

    fn cnam(local: u32, plugin: &str, interner: &StringInterner) -> FieldEntry {
        field("CNAM", FieldValue::FormKey(fk(local, plugin, interner)))
    }

    #[test]
    fn strips_filters_from_item_and_workshop_benches_but_not_cooking() {
        let interner = StringInterner::new();
        let index = index(&interner);
        let armour_bench = || bnam(0x1F6062, "Output.esp", &interner);
        for (name, recipe, filter_count, expected_stripped) in [
            ("armour bench", vec![armour_bench()], 1, true),
            (
                "power armour bench",
                vec![bnam(
                    FO4_POWER_ARMOR_WORKBENCH,
                    FO4_MASTER_PLUGIN,
                    &interner,
                )],
                1,
                true,
            ),
            (
                "no workbench",
                vec![cnam(0x537067, "Output.esp", &interner)],
                1,
                true,
            ),
            (
                "null workbench",
                vec![bnam(0, "Output.esp", &interner)],
                1,
                true,
            ),
            (
                "workshop menu recipe",
                vec![bnam(0x08280B, FO4_MASTER_PLUGIN, &interner)],
                1,
                true,
            ),
            ("every filter entry", vec![armour_bench()], 2, true),
            (
                "cooking bench",
                vec![bnam(0x1F6070, "Output.esp", &interner)],
                1,
                false,
            ),
            ("no filter", vec![armour_bench()], 0, false),
            (
                "armour at the armour bench",
                vec![armour_bench(), cnam(ARMOR, "Output.esp", &interner)],
                1,
                false,
            ),
            (
                "FO4 weapon at the weapon bench",
                vec![
                    bnam(0x1F6063, "Output.esp", &interner),
                    cnam(WEAPON, FO4_MASTER_PLUGIN, &interner),
                ],
                1,
                false,
            ),
            (
                "mod at the armour bench",
                vec![armour_bench(), cnam(0x537067, "Output.esp", &interner)],
                1,
                true,
            ),
            (
                "armour at the power armour bench",
                vec![
                    bnam(FO4_POWER_ARMOR_WORKBENCH, FO4_MASTER_PLUGIN, &interner),
                    cnam(ARMOR, "Output.esp", &interner),
                ],
                1,
                true,
            ),
        ] {
            let mut fields = recipe;
            let kept_len = fields.len();
            fields.extend((0..filter_count).map(|index| fnam(0x47EF39 + index, &interner)));
            let mut record = cobj(fields, &interner);

            assert_eq!(
                strip_recipe_filters(&mut record, &index),
                expected_stripped,
                "{name}"
            );
            let expected_len = if expected_stripped {
                kept_len
            } else {
                kept_len + filter_count as usize
            };
            assert_eq!(record.fields.len(), expected_len, "{name}");
            assert!(
                !strip_recipe_filters(&mut record, &index),
                "{name}: second pass"
            );
        }
    }
}
