use super::*;

pub(super) const FO76_WORKSHOP_WORKBENCH_ALL_TYPE: u32 = 0x89383D;

pub(super) const FO4_WORKSHOP_WORKBENCH_EXTERIOR: u32 = 0x05A0C8;
pub(super) const FO4_WORKSHOP_WORKBENCH_FURNITURE: u32 = 0x05B5E3;
pub(super) const FO4_WORKSHOP_WORKBENCH_DECORATIONS: u32 = 0x08280B;
pub(super) const FO4_WORKSHOP_WORKBENCH_POWER: u32 = 0x05A0CA;
pub(super) const FO4_WORKSHOP_WORKBENCH_CRAFTING: u32 = 0x12E2C8;
pub(super) const FO4_WORKSHOP_WORKBENCH_SETTLEMENT: u32 = 0x246F85;

pub(super) const FO76_WORKSHOP_CATEGORY_APPLIANCES: u32 = 0x04422C;
pub(super) const FO76_WORKSHOP_CATEGORY_BEDS: u32 = 0x04640E;
pub(super) const FO76_WORKSHOP_CATEGORY_BLUEPRINTS: u32 = 0x046411;
pub(super) const FO76_WORKSHOP_CATEGORY_CEILING_DECOR: u32 = 0x4249B6;
pub(super) const FO76_WORKSHOP_CATEGORY_CHAIRS: u32 = 0x04642D;
pub(super) const FO76_WORKSHOP_CATEGORY_CONTAINERS: u32 = 0x046433;
pub(super) const FO76_WORKSHOP_CATEGORY_CRAFTING: u32 = 0x046471;
pub(super) const FO76_WORKSHOP_CATEGORY_DEFENSE: u32 = 0x386054;
pub(super) const FO76_WORKSHOP_CATEGORY_DISPLAYS: u32 = 0x060B21;
pub(super) const FO76_WORKSHOP_CATEGORY_DOORS: u32 = 0x061FA1;
pub(super) const FO76_WORKSHOP_CATEGORY_FLOOR_DECOR: u32 = 0x062011;
pub(super) const FO76_WORKSHOP_CATEGORY_FLOORS: u32 = 0x06201B;
pub(super) const FO76_WORKSHOP_CATEGORY_FOOD: u32 = 0x06201D;
pub(super) const FO76_WORKSHOP_CATEGORY_GENERATORS: u32 = 0x06201E;
pub(super) const FO76_WORKSHOP_CATEGORY_LIGHTS: u32 = 0x06201F;
pub(super) const FO76_WORKSHOP_CATEGORY_MISC_STRUCTURES: u32 = 0x11C420;
pub(super) const FO76_WORKSHOP_CATEGORY_POWER_CONNECTORS: u32 = 0x08032C;
pub(super) const FO76_WORKSHOP_CATEGORY_RESOURCES: u32 = 0x095A38;
pub(super) const FO76_WORKSHOP_CATEGORY_ROOFS: u32 = 0x12882C;
pub(super) const FO76_WORKSHOP_CATEGORY_SHELTERS: u32 = 0x5A60A0;
pub(super) const FO76_WORKSHOP_CATEGORY_SHELVES: u32 = 0x12882D;
pub(super) const FO76_WORKSHOP_CATEGORY_STAIRS: u32 = 0x12882E;
pub(super) const FO76_WORKSHOP_CATEGORY_TABLES: u32 = 0x1573C7;
pub(super) const FO76_WORKSHOP_CATEGORY_TURRETS_TRAPS: u32 = 0x05294F;
pub(super) const FO76_WORKSHOP_CATEGORY_VENDORS: u32 = 0x12882F;
pub(super) const FO76_WORKSHOP_CATEGORY_WALL_DECOR: u32 = 0x1573EE;
pub(super) const FO76_WORKSHOP_CATEGORY_WALLS: u32 = 0x1573F0;
pub(super) const FO76_WORKSHOP_CATEGORY_WATER: u32 = 0x1573F7;
pub(super) const FO76_WORKSHOP_CATEGORY_DWELLERS: u32 = 0x54EB71;
pub(super) const FO76_WORKSHOP_CATEGORY_PETS: u32 = 0x411B84;
pub(super) const FO76_WORKSHOP_CATEGORY_QUEST: u32 = 0x5895EE;
pub(super) const FO76_WORKSHOP_CATEGORY_MAIN_STRUCTURE: u32 = 0x8229E6;
pub(super) const FO76_WORKSHOP_CATEGORY_MAIN_FURNITURE: u32 = 0x8229E2;
pub(super) const FO76_WORKSHOP_CATEGORY_MAIN_DECORATIONS: u32 = 0x8229E3;
pub(super) const FO76_WORKSHOP_CATEGORY_MAIN_DEFENSE: u32 = 0x8229DF;
pub(super) const FO76_WORKSHOP_CATEGORY_MAIN_POWER: u32 = 0x8229E0;
pub(super) const FO76_WORKSHOP_CATEGORY_MAIN_RESOURCES: u32 = 0x8229E1;
pub(super) const FO76_WORKSHOP_CATEGORY_MAIN_STORAGE: u32 = 0x8229E5;
pub(super) const FO76_WORKSHOP_CATEGORY_MAIN_UTILITY: u32 = 0x822A19;
pub(super) const FO76_WORKSHOP_CATEGORY_MAIN_DWELLERS: u32 = 0x8229E7;
pub(super) const FO76_WORKSHOP_CATEGORY_MAIN_QUEST: u32 = 0x8229DD;
impl Fo76Fo4Hook {
    pub(super) fn is_convertible_workshop_cobj(
        interner: &crate::sym::StringInterner,
        record: &Record,
    ) -> bool {
        if record.sig.0 != *b"COBJ" {
            return false;
        }
        let Some(eid) = record.eid.and_then(|eid| interner.resolve(eid)) else {
            return false;
        };
        let eid = eid.to_ascii_lowercase();
        let has_workshop_token = eid.starts_with("workshop_") || eid.contains("_workshop_");
        let excluded_recipe_family = ["co_mod", "co_clothes", "co_cloths"]
            .iter()
            .any(|family| eid.starts_with(family) || eid.contains(&format!("_{family}")));
        has_workshop_token && !eid.starts_with("zzz") && !excluded_recipe_family
    }

    pub(super) fn workshop_category_workbench(category: u32) -> Option<u32> {
        match category {
            FO76_WORKSHOP_CATEGORY_APPLIANCES
            | FO76_WORKSHOP_CATEGORY_BEDS
            | FO76_WORKSHOP_CATEGORY_CHAIRS
            | FO76_WORKSHOP_CATEGORY_CONTAINERS
            | FO76_WORKSHOP_CATEGORY_DISPLAYS
            | FO76_WORKSHOP_CATEGORY_SHELVES
            | FO76_WORKSHOP_CATEGORY_TABLES
            | FO76_WORKSHOP_CATEGORY_MAIN_FURNITURE
            | FO76_WORKSHOP_CATEGORY_MAIN_STORAGE => Some(FO4_WORKSHOP_WORKBENCH_FURNITURE),
            FO76_WORKSHOP_CATEGORY_CEILING_DECOR
            | FO76_WORKSHOP_CATEGORY_FLOOR_DECOR
            | FO76_WORKSHOP_CATEGORY_WALL_DECOR
            | FO76_WORKSHOP_CATEGORY_MAIN_DECORATIONS => Some(FO4_WORKSHOP_WORKBENCH_DECORATIONS),
            FO76_WORKSHOP_CATEGORY_GENERATORS
            | FO76_WORKSHOP_CATEGORY_LIGHTS
            | FO76_WORKSHOP_CATEGORY_POWER_CONNECTORS
            | FO76_WORKSHOP_CATEGORY_MAIN_POWER => Some(FO4_WORKSHOP_WORKBENCH_POWER),
            FO76_WORKSHOP_CATEGORY_CRAFTING | FO76_WORKSHOP_CATEGORY_MAIN_UTILITY => {
                Some(FO4_WORKSHOP_WORKBENCH_CRAFTING)
            }
            FO76_WORKSHOP_CATEGORY_RESOURCES
            | FO76_WORKSHOP_CATEGORY_VENDORS
            | FO76_WORKSHOP_CATEGORY_DWELLERS
            | FO76_WORKSHOP_CATEGORY_PETS
            | FO76_WORKSHOP_CATEGORY_MAIN_DWELLERS => Some(FO4_WORKSHOP_WORKBENCH_SETTLEMENT),
            FO76_WORKSHOP_CATEGORY_BLUEPRINTS
            | FO76_WORKSHOP_CATEGORY_DEFENSE
            | FO76_WORKSHOP_CATEGORY_DOORS
            | FO76_WORKSHOP_CATEGORY_FLOORS
            | FO76_WORKSHOP_CATEGORY_FOOD
            | FO76_WORKSHOP_CATEGORY_MISC_STRUCTURES
            | FO76_WORKSHOP_CATEGORY_ROOFS
            | FO76_WORKSHOP_CATEGORY_SHELTERS
            | FO76_WORKSHOP_CATEGORY_STAIRS
            | FO76_WORKSHOP_CATEGORY_TURRETS_TRAPS
            | FO76_WORKSHOP_CATEGORY_WALLS
            | FO76_WORKSHOP_CATEGORY_WATER
            | FO76_WORKSHOP_CATEGORY_QUEST
            | FO76_WORKSHOP_CATEGORY_MAIN_STRUCTURE
            | FO76_WORKSHOP_CATEGORY_MAIN_DEFENSE
            | FO76_WORKSHOP_CATEGORY_MAIN_RESOURCES
            | FO76_WORKSHOP_CATEGORY_MAIN_QUEST => Some(FO4_WORKSHOP_WORKBENCH_EXTERIOR),
            _ => None,
        }
    }

    pub(super) fn infer_workshop_category(eid: &str) -> (u32, u32) {
        let eid = eid.to_ascii_lowercase();
        if eid.contains("campfire")
            || eid.contains("camp_fire")
            || eid.contains("firebarrel")
            || eid.contains("fire_barrel")
        {
            return (
                FO76_WORKSHOP_CATEGORY_LIGHTS,
                FO4_WORKSHOP_WORKBENCH_FURNITURE,
            );
        }
        let category = if eid.contains("walldecor") || eid.contains("wall_decor") {
            FO76_WORKSHOP_CATEGORY_WALL_DECOR
        } else if eid.contains("floordecor") || eid.contains("floor_decor") {
            FO76_WORKSHOP_CATEGORY_FLOOR_DECOR
        } else if eid.contains("ceilingdecor") || eid.contains("ceiling_decor") {
            FO76_WORKSHOP_CATEGORY_CEILING_DECOR
        } else if eid.contains("generator") {
            FO76_WORKSHOP_CATEGORY_GENERATORS
        } else if eid.contains("powerconnector") || eid.contains("power_connector") {
            FO76_WORKSHOP_CATEGORY_POWER_CONNECTORS
        } else if eid.contains("light") || eid.contains("lamp") {
            FO76_WORKSHOP_CATEGORY_LIGHTS
        } else if eid.contains("crafting") || eid.contains("workbench") {
            FO76_WORKSHOP_CATEGORY_CRAFTING
        } else if eid.contains("vendor") || eid.contains("vending") {
            FO76_WORKSHOP_CATEGORY_VENDORS
        } else if eid.contains("display") {
            FO76_WORKSHOP_CATEGORY_DISPLAYS
        } else if eid.contains("container") || eid.contains("stash") {
            FO76_WORKSHOP_CATEGORY_CONTAINERS
        } else if eid.contains("shelf") {
            FO76_WORKSHOP_CATEGORY_SHELVES
        } else if eid.contains("table") {
            FO76_WORKSHOP_CATEGORY_TABLES
        } else if eid.contains("chair") || eid.contains("seating") {
            FO76_WORKSHOP_CATEGORY_CHAIRS
        } else if eid.contains("bed") {
            FO76_WORKSHOP_CATEGORY_BEDS
        } else if eid.contains("appliance") || eid.contains("utility") {
            FO76_WORKSHOP_CATEGORY_APPLIANCES
        } else if eid.contains("turret") || eid.contains("trap") || eid.contains("defense") {
            FO76_WORKSHOP_CATEGORY_TURRETS_TRAPS
        } else if eid.contains("water") {
            FO76_WORKSHOP_CATEGORY_WATER
        } else if eid.contains("food") || eid.contains("crop") || eid.contains("plant") {
            FO76_WORKSHOP_CATEGORY_FOOD
        } else if eid.contains("resource") || eid.contains("collector") || eid.contains("producer")
        {
            FO76_WORKSHOP_CATEGORY_RESOURCES
        } else if eid.contains("door") {
            FO76_WORKSHOP_CATEGORY_DOORS
        } else if eid.contains("roof") {
            FO76_WORKSHOP_CATEGORY_ROOFS
        } else if eid.contains("stair") {
            FO76_WORKSHOP_CATEGORY_STAIRS
        } else if eid.contains("floor") || eid.contains("foundation") {
            FO76_WORKSHOP_CATEGORY_FLOORS
        } else if eid.contains("wall") {
            FO76_WORKSHOP_CATEGORY_WALLS
        } else if eid.contains("shelter") {
            FO76_WORKSHOP_CATEGORY_SHELTERS
        } else if eid.contains("pet") {
            FO76_WORKSHOP_CATEGORY_PETS
        } else if eid.contains("dweller") || eid.contains("ally") {
            FO76_WORKSHOP_CATEGORY_DWELLERS
        } else if eid.contains("quest") {
            FO76_WORKSHOP_CATEGORY_QUEST
        } else if eid.contains("decor") || eid.contains("rug") || eid.contains("statue") {
            FO76_WORKSHOP_CATEGORY_FLOOR_DECOR
        } else {
            FO76_WORKSHOP_CATEGORY_MISC_STRUCTURES
        };
        let workbench =
            Self::workshop_category_workbench(category).unwrap_or(FO4_WORKSHOP_WORKBENCH_EXTERIOR);
        (category, workbench)
    }

    pub(super) fn normalize_workshop_cobj(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if !Self::is_convertible_workshop_cobj(interner, record) {
            return;
        }
        let has_created_object = record.fields.iter().any(|entry| {
            entry.sig.0 == *b"CNAM"
                && matches!(&entry.value, FieldValue::FormKey(form_key) if form_key.local != 0)
        });
        if !has_created_object {
            return;
        }
        let Some(bench_index) = record
            .fields
            .iter()
            .position(|entry| entry.sig.0 == *b"BNAM")
        else {
            return;
        };
        let FieldValue::FormKey(source_bench) = record.fields[bench_index].value else {
            return;
        };
        let source_plugin = interner.resolve(source_bench.plugin).unwrap_or_default();
        if !source_plugin.eq_ignore_ascii_case(FO76_MASTER_NAME) {
            return;
        }

        let eid = record
            .eid
            .and_then(|eid| interner.resolve(eid))
            .unwrap_or_default();
        let (category, workbench) = if source_bench.local == FO76_WORKSHOP_WORKBENCH_ALL_TYPE {
            Self::infer_workshop_category(eid)
        } else {
            let Some(workbench) = Self::workshop_category_workbench(source_bench.local) else {
                return;
            };
            (source_bench.local, workbench)
        };

        let category_form_key = FormKey {
            local: category,
            plugin: interner.intern(FO76_MASTER_NAME),
        };
        record.fields[bench_index].value = FieldValue::FormKey(FormKey {
            local: workbench,
            plugin: interner.intern(FO4_MASTER_NAME),
        });
        if record.fields.iter().all(|entry| entry.sig.0 != *b"FNAM") {
            record.fields.insert(
                bench_index + 1,
                FieldEntry {
                    sig: SubrecordSig(*b"FNAM"),
                    value: FieldValue::List(vec![FieldValue::FormKey(category_form_key)]),
                },
            );
        }
    }
}
