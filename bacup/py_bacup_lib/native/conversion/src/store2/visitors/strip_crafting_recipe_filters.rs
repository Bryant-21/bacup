//! Sweep adapter for `strip_crafting_recipe_filters` (decoded lane).

use std::any::Any;

use rustc_hash::FxHashSet;

use crate::fixups::strip_crafting_recipe_filters::{
    collect_item_crafting_workbenches, strip_recipe_filters,
};
use crate::fixups::{FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::Record;
use crate::session::PluginSession;
use crate::store2::visitor::{
    GatherOutput, Lane, MasterScanCache, RecordVisitor, SweepCtx, VisitOutcome,
};
use crate::sym::Sym;

pub struct StripCraftingRecipeFiltersVisitor;

impl RecordVisitor for StripCraftingRecipeFiltersVisitor {
    fn name(&self) -> &'static str {
        "strip_crafting_recipe_filters" // == legacy Fixup::name()
    }

    fn lane(&self) -> Lane {
        Lane::Decoded
    }

    fn applies(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        session.target_slot().parsed.game.as_deref() == Some("fo4")
    }

    fn gather(
        &self,
        session: &mut PluginSession,
        mapper: &FormKeyMapper,
        _config: &FixupConfig,
        _master_cache: &mut MasterScanCache,
    ) -> Result<GatherOutput, FixupError> {
        let mut report = FixupReport::empty();
        let benches = collect_item_crafting_workbenches(session, mapper.interner, &mut report)?;
        Ok(GatherOutput {
            candidate_sigs: vec![
                SigCode::from_str("COBJ").map_err(|e| FixupError::SchemaError(e.to_string()))?,
            ],
            index: Some(Box::new(benches)),
            warnings: report.warnings,
        })
    }

    fn visit_decoded(
        &self,
        record: &mut Record,
        index: Option<&(dyn Any + Send + Sync)>,
        _cx: &SweepCtx<'_>,
        _warnings: &mut Vec<Sym>,
    ) -> VisitOutcome {
        let benches = index
            .and_then(|i| i.downcast_ref::<FxHashSet<FormKey>>())
            .expect("item-crafting workbench index");
        if strip_recipe_filters(record, benches) {
            VisitOutcome::Changed
        } else {
            VisitOutcome::Unchanged
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::strip_crafting_recipe_filters::StripCraftingRecipeFiltersFixup;
    use crate::ids::SubrecordSig;
    use crate::record::{FieldEntry, FieldValue, RecordFlags};
    use crate::store2::test_util::assert_handles_equal;
    use crate::store2::visitors::port_test_util::*;
    use smallvec::SmallVec;

    const ARMOR_BENCH: u32 = 0x1F6062;
    const COOKING_BENCH: u32 = 0x1F6070;

    fn field(sig: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        }
    }

    fn rec(
        sig: &str,
        local: u32,
        eid: &str,
        extra: Vec<FieldEntry>,
        interner: &crate::sym::StringInterner,
    ) -> Record {
        let eid_sym = interner.intern(eid);
        let mut fields: SmallVec<[FieldEntry; 8]> = smallvec::smallvec![FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(eid_sym),
        }];
        fields.extend(extra);
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern("CraftFilter.esp"),
            },
            eid: Some(eid_sym),
            flags: RecordFlags::empty(),
            fields,
            warnings: SmallVec::new(),
        }
    }

    fn bench_link(local: u32, interner: &crate::sym::StringInterner) -> FieldEntry {
        field(
            "BNAM",
            FieldValue::FormKey(FormKey {
                local,
                plugin: interner.intern("CraftFilter.esp"),
            }),
        )
    }

    fn category(local: u32, interner: &crate::sym::StringInterner) -> FieldEntry {
        field(
            "FNAM",
            FieldValue::List(vec![FieldValue::FormKey(FormKey {
                local,
                plugin: interner.intern("CraftFilter.esp"),
            })]),
        )
    }

    /// Fixture: the armour bench keyword the gather resolves by EditorID, a
    /// cooking bench it must not, plus one recipe on each and one with no
    /// bench at all (the OMOD case).
    #[test]
    fn visitor_matches_legacy_fixup() {
        let (h_old, h_new) = seed_twin("CraftFilter.esp", |session, schema, interner| {
            let records = [
                rec(
                    "KYWD",
                    ARMOR_BENCH,
                    "Workbench_Crafting_Armor",
                    vec![],
                    interner,
                ),
                rec(
                    "KYWD",
                    COOKING_BENCH,
                    "Workbench_Crafting_Cooking",
                    vec![],
                    interner,
                ),
                rec(
                    "KYWD",
                    0x47EF39,
                    "RecipeFilter_Armor_Backpack",
                    vec![],
                    interner,
                ),
                rec(
                    "COBJ",
                    0x801,
                    "ATX_co_Armor_Backpack_HeirloomBasket",
                    vec![
                        bench_link(ARMOR_BENCH, interner),
                        category(0x47EF39, interner),
                    ],
                    interner,
                ),
                rec(
                    "COBJ",
                    0x802,
                    "ATX_co_mod_PowerArmor_Excavator_Torso_Paint_Fire",
                    vec![category(0x47EF39, interner)],
                    interner,
                ),
                rec(
                    "COBJ",
                    0x803,
                    "SCORE_25_co_meal_TatoSaladCooked_Cannery",
                    vec![
                        bench_link(COOKING_BENCH, interner),
                        category(0x47EF39, interner),
                    ],
                    interner,
                ),
            ];
            for r in records {
                session.add_record(r, schema.as_ref(), interner).unwrap();
            }
        });

        let config = config_for(h_old);
        let legacy = run_legacy_fixup(h_old, Box::new(StripCraftingRecipeFiltersFixup), &config);
        let v2 = run_visitor_sweep(
            h_new,
            "crafting_filters",
            vec![Box::new(StripCraftingRecipeFiltersVisitor)],
            &config,
        );

        assert_changed_parity(&legacy, &v2);
        assert_eq!(
            v2[0].1.records_changed, 2,
            "armour-bench and bench-less recipes stripped; the cooking recipe kept"
        );
        assert_handles_equal(h_old, h_new);
    }
}
