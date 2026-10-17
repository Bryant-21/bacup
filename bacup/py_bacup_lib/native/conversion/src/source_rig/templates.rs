use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{
    CreatureNpcInventoryOwnership, CreatureRecordProjectionManifest, CreatureTargetRecordReference,
    TargetFormKey,
};
use crate::ids::{SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::sym::StringInterner;
use crate::target_normalize::{TargetRecordNormalization, TargetRecordNormalizer};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CreatureTemplateProjection {
    pub wrapper: TargetFormKey,
    pub template: CreatureTargetRecordReference,
    pub template_flags: u16,
    pub leveled_lists: Vec<CreatureLeveledNpcProjection>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CreatureLeveledNpcProjection {
    pub form_key: TargetFormKey,
    pub editor_id: String,
    pub chance_none: u8,
    pub flags: u8,
    pub global: Option<TargetFormKey>,
    pub entries: Vec<CreatureLeveledNpcEntry>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CreatureLeveledNpcEntry {
    pub level: u16,
    pub count: u16,
    pub reference: CreatureTargetRecordReference,
    pub ownership: Option<CreatureNpcInventoryOwnership>,
}

impl CreatureTemplateProjection {
    pub(crate) fn dependencies(&self) -> Vec<CreatureTargetRecordReference> {
        let mut dependencies = vec![self.template.clone()];
        for list in &self.leveled_lists {
            dependencies.extend(list.global.iter().cloned().map(|form_key| {
                CreatureTargetRecordReference {
                    signature: "GLOB".into(),
                    form_key,
                }
            }));
            for entry in &list.entries {
                dependencies.push(entry.reference.clone());
                match &entry.ownership {
                    Some(CreatureNpcInventoryOwnership::FactionRank { faction, .. }) => {
                        dependencies.push(faction.clone())
                    }
                    Some(CreatureNpcInventoryOwnership::OwnerGlobal { owner, global, .. }) => {
                        dependencies.extend(owner.iter().cloned());
                        dependencies.extend(global.iter().cloned());
                    }
                    None => {}
                }
            }
        }
        dependencies
    }

    pub(crate) fn validate(
        &self,
        projection: &CreatureRecordProjectionManifest,
    ) -> Result<(), String> {
        if self.template_flags != 0x1fbf
            || self.template.signature != "LVLN"
            || !projection
                .variants
                .iter()
                .any(|variant| variant.primary && variant.form_key == self.wrapper)
        {
            return Err("template wrapper must be the primary NPC and inherit every consumed category from one LVLN selection".into());
        }
        let mut keys = BTreeSet::new();
        for list in &self.leveled_lists {
            if list.form_key.local == 0
                || list.form_key.local > 0xffffff
                || !list
                    .form_key
                    .plugin
                    .eq_ignore_ascii_case(&projection.base.target_plugin)
                || !keys.insert(list.form_key.clone())
                || list.entries.is_empty()
                || list.entries.len() > 255
                || list.chance_none > 100
                || !list
                    .editor_id
                    .starts_with(&projection.base.editor_id_prefix)
                || list
                    .entries
                    .iter()
                    .any(|entry| !matches!(entry.reference.signature.as_str(), "NPC_" | "LVLN"))
            {
                return Err(
                    "generated template list identity or ordered entries are invalid".into(),
                );
            }
        }
        if !keys.contains(&self.template.form_key) {
            return Err("template wrapper root LVLN is absent from its owned list records".into());
        }
        Ok(())
    }

    pub(crate) fn apply(
        &self,
        records: &mut Vec<Record>,
        interner: &StringInterner,
    ) -> Result<(), String> {
        let wrapper = records
            .iter_mut()
            .find(|record| {
                record.sig.as_str() == "NPC_" && record.form_key == self.wrapper.intern(interner)
            })
            .ok_or("template wrapper NPC is absent")?;
        let acbs = wrapper
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "ACBS")
            .ok_or("template wrapper ACBS is absent")?;
        let FieldValue::Struct(fields) = &mut acbs.value else {
            return Err("template wrapper ACBS is not typed".into());
        };
        for (name, value) in fields {
            match interner.resolve(*name) {
                Some("flags") => {
                    let FieldValue::Uint(flags) = value else {
                        return Err("template wrapper actor flags are not typed".into());
                    };
                    *flags = (*flags & !0x200) | 0x100;
                }
                Some("template_flags") => *value = FieldValue::Uint(self.template_flags.into()),
                _ => {}
            }
        }
        wrapper.fields.push(field(
            "TPLT",
            FieldValue::FormKey(self.template.form_key.intern(interner)),
        ));
        records.extend(self.leveled_lists.iter().map(|list| list.emit(interner)));
        Ok(())
    }

    pub(crate) fn verify(
        &self,
        records: &[Record],
        interner: &StringInterner,
    ) -> Result<(), String> {
        let wrapper = records
            .iter()
            .find(|record| {
                record.sig.as_str() == "NPC_" && record.form_key == self.wrapper.intern(interner)
            })
            .ok_or("template wrapper NPC is absent after normalization")?;
        if !wrapper.fields.iter().any(|entry| {
            entry.sig.as_str() == "TPLT"
                && entry.value == FieldValue::FormKey(self.template.form_key.intern(interner))
        }) {
            return Err("template wrapper lost native TPLT".into());
        }
        let Some(FieldValue::Struct(acbs)) = wrapper
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "ACBS")
            .map(|entry| &entry.value)
        else {
            return Err("template wrapper lost typed ACBS".into());
        };
        if !acbs.iter().any(|(name, value)| {
            interner.resolve(*name) == Some("template_flags")
                && scalar_u64(value) == Some(self.template_flags.into())
        }) || !acbs.iter().any(|(name, value)| {
            interner.resolve(*name) == Some("flags")
                && scalar_u64(value).is_some_and(|flags| flags & 0x300 == 0x100)
        }) {
            return Err(
                "template wrapper no longer shares one native selection across categories".into(),
            );
        }
        let schema = AuthoringSchema::for_game("fo4")?;
        let normalizer = TargetRecordNormalizer::target_only_with_interner(&schema, interner);
        for list in &self.leveled_lists {
            let TargetRecordNormalization::Keep(mut expected) =
                normalizer.normalize(list.emit(interner))
            else {
                return Err("target schema dropped generated LVLN".into());
            };
            super::records::restore_entry_ownership_associations(
                &mut expected,
                "LVLO",
                &list
                    .entries
                    .iter()
                    .map(|entry| entry.ownership.is_some())
                    .collect::<Vec<_>>(),
            )
            .map_err(|error| error.to_string())?;
            let actual = records
                .iter()
                .find(|record| {
                    record.sig.as_str() == "LVLN" && record.form_key == expected.form_key
                })
                .ok_or("generated template LVLN is absent")?;
            if actual.fields != expected.fields {
                return Err("generated LVLN changed its source entry order, multiplicity, ownership or selection fields".into());
            }
        }
        Ok(())
    }
}

fn scalar_u64(value: &FieldValue) -> Option<u64> {
    match value {
        FieldValue::Uint(value) => Some(*value),
        FieldValue::Bytes(bytes) if matches!(bytes.len(), 1 | 2 | 4 | 8) => {
            let mut value = [0u8; 8];
            value[..bytes.len()].copy_from_slice(bytes);
            Some(u64::from_le_bytes(value))
        }
        _ => None,
    }
}

impl CreatureLeveledNpcProjection {
    fn emit(&self, interner: &StringInterner) -> Record {
        let mut record = Record::new(
            SigCode::from_str("LVLN").unwrap(),
            self.form_key.intern(interner),
        );
        record.eid = Some(interner.intern(&self.editor_id));
        record.fields.extend([
            field("EDID", FieldValue::String(interner.intern(&self.editor_id))),
            field("OBND", FieldValue::Bytes(vec![0; 12].into())),
            field("LVLD", FieldValue::Uint(self.chance_none.into())),
            field("LVLF", FieldValue::Uint(self.flags.into())),
        ]);
        if let Some(global) = &self.global {
            record
                .fields
                .push(field("LVLG", FieldValue::FormKey(global.intern(interner))));
        }
        record
            .fields
            .push(field("LLCT", FieldValue::Uint(self.entries.len() as u64)));
        for entry in &self.entries {
            record.fields.push(field(
                "LVLO",
                object(
                    interner,
                    [
                        ("level", FieldValue::Uint(entry.level.into())),
                        ("unknown_u8_1", FieldValue::Uint(0)),
                        ("unknown_u8_2", FieldValue::Uint(0)),
                        (
                            "npc",
                            FieldValue::FormKey(entry.reference.form_key.intern(interner)),
                        ),
                        ("count", FieldValue::Uint(entry.count.into())),
                        ("chance_none", FieldValue::Uint(0)),
                        ("unknown_u8_6", FieldValue::Uint(0)),
                    ],
                ),
            ));
            if let Some(ownership) = &entry.ownership {
                let (owner, global_or_rank, condition) = match ownership {
                    CreatureNpcInventoryOwnership::FactionRank {
                        faction,
                        required_rank,
                        condition,
                    } => (
                        FieldValue::FormKey(faction.form_key.intern(interner)),
                        FieldValue::Int((*required_rank).into()),
                        *condition,
                    ),
                    CreatureNpcInventoryOwnership::OwnerGlobal {
                        owner,
                        global,
                        condition,
                    } => (
                        owner.as_ref().map_or(FieldValue::Uint(0), |value| {
                            FieldValue::FormKey(value.form_key.intern(interner))
                        }),
                        global.as_ref().map_or(FieldValue::Uint(0), |value| {
                            FieldValue::FormKey(value.form_key.intern(interner))
                        }),
                        *condition,
                    ),
                };
                record.fields.push(field(
                    "COED",
                    object(
                        interner,
                        [
                            ("owner", owner),
                            ("global_variable_required_rank", global_or_rank),
                            ("item_condition", FieldValue::Float(condition)),
                        ],
                    ),
                ));
            }
        }
        record
    }
}

fn field(signature: &str, value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig::from_str(signature).unwrap(),
        value,
    }
}

fn object<const N: usize>(
    interner: &StringInterner,
    values: [(&str, FieldValue); N],
) -> FieldValue {
    FieldValue::Struct(
        values
            .into_iter()
            .map(|(name, value)| (interner.intern(name), value))
            .collect(),
    )
}
