use rustc_hash::{FxHashMap, FxHashSet};

use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldValue, Record};
use crate::sym::{StringInterner, Sym};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningPolicy {
    WarnPlayable,
}

impl Default for WarningPolicy {
    fn default() -> Self {
        Self::WarnPlayable
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixupScope {
    WholePluginSafe,
    GraphOnly,
    AssetOnly,
    DisabledPending,
    /// Runs on whole-plugin (NOT skipped like `GraphOnly`) but the fixup
    /// self-gates each record on the creature predicate, so it touches only
    /// actual creatures. Used by the creature-internal fixups.
    CreatureGated,
}

impl FixupScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WholePluginSafe => "whole_plugin_safe",
            Self::GraphOnly => "graph_only",
            Self::AssetOnly => "asset_only",
            Self::DisabledPending => "disabled_pending",
            Self::CreatureGated => "creature_gated",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixupStatus {
    Ran,
    Skipped,
}

impl FixupStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ran => "ran",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AssetPhaseFlags {
    pub terrain: bool,
    pub nifs: bool,
    pub textures: bool,
    pub materials: bool,
    pub havok: bool,
    pub animations: bool,
    pub sounds: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RefOwner {
    pub owner: FormKey,
    pub owner_sig: SigCode,
}

#[derive(Debug, Clone, Default)]
pub struct FullPluginRunState {
    pub translated_by_signature: FxHashMap<SigCode, u32>,
    pub unresolved_source_ref_owners: FxHashMap<FormKey, Vec<RefOwner>>,
    pub target_master_ref_owners: FxHashMap<FormKey, Vec<RefOwner>>,
    seen_unresolved_pairs: FxHashSet<(FormKey, FormKey)>,
    seen_target_master_pairs: FxHashSet<(FormKey, FormKey)>,
}

impl FullPluginRunState {
    pub fn record_translated(&mut self, sig: SigCode) {
        *self.translated_by_signature.entry(sig).or_insert(0) += 1;
    }

    pub fn capture_record_refs(
        &mut self,
        record: &Record,
        source_plugin: Sym,
        target_master_plugins: &FxHashSet<Sym>,
    ) {
        let owner = RefOwner {
            owner: record.form_key,
            owner_sig: record.sig,
        };
        for entry in &record.fields {
            self.capture_value_refs(&entry.value, owner, source_plugin, target_master_plugins);
        }
    }

    pub fn capture_raw_zero_master_refs(&mut self, record: &Record, raw_zero_master_plugin: Sym) {
        let owner = RefOwner {
            owner: record.form_key,
            owner_sig: record.sig,
        };
        for entry in &record.fields {
            if !is_raw_zero_master_ref_sig(entry.sig) {
                continue;
            }
            let FieldValue::Bytes(bytes) = &entry.value else {
                continue;
            };
            let Some(raw) = raw_zero_master_ref_at(bytes, 0) else {
                continue;
            };
            let target_master_ref = FormKey {
                local: raw & 0x00FF_FFFF,
                plugin: raw_zero_master_plugin,
            };
            if self
                .seen_target_master_pairs
                .insert((owner.owner, target_master_ref))
            {
                self.target_master_ref_owners
                    .entry(target_master_ref)
                    .or_default()
                    .push(owner);
            }
        }
    }

    fn capture_value_refs(
        &mut self,
        value: &FieldValue,
        owner: RefOwner,
        source_plugin: Sym,
        target_master_plugins: &FxHashSet<Sym>,
    ) {
        match value {
            FieldValue::FormKey(fk) if fk.local != 0 && fk.plugin == source_plugin => {
                if self.seen_unresolved_pairs.insert((owner.owner, *fk)) {
                    self.unresolved_source_ref_owners
                        .entry(*fk)
                        .or_default()
                        .push(owner);
                }
            }
            FieldValue::FormKey(fk)
                if fk.local != 0 && target_master_plugins.contains(&fk.plugin) =>
            {
                if self.seen_target_master_pairs.insert((owner.owner, *fk)) {
                    self.target_master_ref_owners
                        .entry(*fk)
                        .or_default()
                        .push(owner);
                }
            }
            FieldValue::List(values) => {
                for child in values {
                    self.capture_value_refs(child, owner, source_plugin, target_master_plugins);
                }
            }
            FieldValue::Struct(fields) => {
                for (_name, child) in fields {
                    self.capture_value_refs(child, owner, source_plugin, target_master_plugins);
                }
            }
            _ => {}
        }
    }

    pub fn unresolved_ref_count(&self) -> usize {
        self.unresolved_source_ref_owners.len()
    }

    pub fn target_master_ref_count(&self) -> usize {
        self.target_master_ref_owners.len()
    }
}

fn is_raw_zero_master_ref_sig(sig: SubrecordSig) -> bool {
    matches!(&sig.0, b"XLCN" | b"XEZN" | b"XAPR")
}

fn raw_zero_master_ref_at(bytes: &[u8], offset: usize) -> Option<u32> {
    let chunk = bytes.get(offset..offset.checked_add(4)?)?;
    let raw = u32::from_le_bytes(chunk.try_into().ok()?);
    (raw != 0 && raw >> 24 == 0).then_some(raw)
}

pub fn intern_plugin_names(names: &[String], interner: &StringInterner) -> FxHashSet<Sym> {
    names.iter().map(|name| interner.intern(name)).collect()
}

pub fn target_schema_record_view(
    record: &Record,
    target_schema: &crate::schema::AuthoringSchema,
) -> Record {
    let mut view = record.clone();

    if let Some(record_def) = target_schema.record_def(view.sig.as_str()) {
        view.fields
            .retain(|entry| record_def.subrecord_def(entry.sig.as_str()).is_some());
    }

    view
}

pub fn persisted_target_record_view(
    record: &Record,
    target_schema: &crate::schema::AuthoringSchema,
    interner: &StringInterner,
    output_plugin_name: &str,
    target_master_plugins: &FxHashSet<Sym>,
) -> Record {
    let mut view = target_schema_record_view(record, target_schema);

    let output_plugin = interner.intern(output_plugin_name);
    for entry in &mut view.fields {
        normalize_persisted_formkeys(&mut entry.value, output_plugin, target_master_plugins);
    }

    view
}

fn normalize_persisted_formkeys(
    value: &mut FieldValue,
    output_plugin: Sym,
    target_master_plugins: &FxHashSet<Sym>,
) {
    match value {
        FieldValue::FormKey(fk) if fk.local != 0 && !target_master_plugins.contains(&fk.plugin) => {
            fk.plugin = output_plugin;
        }
        FieldValue::List(values) => {
            for child in values {
                normalize_persisted_formkeys(child, output_plugin, target_master_plugins);
            }
        }
        FieldValue::Struct(fields) => {
            for (_name, child) in fields {
                normalize_persisted_formkeys(child, output_plugin, target_master_plugins);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::schema::AuthoringSchema;
    use smallvec::smallvec;

    #[test]
    fn capture_record_refs_records_unresolved_source_refs() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("Source.esp");
        let target_master_plugin = interner.intern("Fallout4.esm");
        let output_plugin = interner.intern("Output.esp");
        let owner = FormKey {
            local: 0x800,
            plugin: output_plugin,
        };
        let source_ref = FormKey {
            local: 0x801,
            plugin: source_plugin,
        };
        let target_master_ref = FormKey {
            local: 0x802,
            plugin: target_master_plugin,
        };
        let owner_sig = SigCode::from_str("WEAP").unwrap();
        let nested_name = interner.intern("nested");
        let record = Record {
            sig: owner_sig,
            form_key: owner,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec![
                FieldEntry {
                    sig: SubrecordSig::from_str("AMMO").unwrap(),
                    value: FieldValue::FormKey(source_ref),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("DNAM").unwrap(),
                    value: FieldValue::Struct(vec![(
                        nested_name,
                        FieldValue::FormKey(target_master_ref),
                    )]),
                },
            ],
            warnings: smallvec![],
        };
        let mut target_master_plugins = FxHashSet::default();
        target_master_plugins.insert(target_master_plugin);
        let mut state = FullPluginRunState::default();

        state.capture_record_refs(&record, source_plugin, &target_master_plugins);

        assert_eq!(state.unresolved_ref_count(), 1);
        assert_eq!(state.target_master_ref_count(), 1);
        assert_eq!(
            state.unresolved_source_ref_owners.get(&source_ref).unwrap(),
            &vec![RefOwner { owner, owner_sig }]
        );
        assert_eq!(
            state
                .target_master_ref_owners
                .get(&target_master_ref)
                .unwrap(),
            &vec![RefOwner { owner, owner_sig }]
        );
    }

    #[test]
    fn capture_record_refs_deduplicates_owner_reference_pairs() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("Source.esp");
        let output_plugin = interner.intern("Output.esp");
        let owner = FormKey {
            local: 0x800,
            plugin: output_plugin,
        };
        let source_ref = FormKey {
            local: 0x801,
            plugin: source_plugin,
        };
        let owner_sig = SigCode::from_str("WEAP").unwrap();
        let nested_name = interner.intern("nested");
        let record = Record {
            sig: owner_sig,
            form_key: owner,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec![
                FieldEntry {
                    sig: SubrecordSig::from_str("AMMO").unwrap(),
                    value: FieldValue::FormKey(source_ref),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("DNAM").unwrap(),
                    value: FieldValue::Struct(vec![(nested_name, FieldValue::FormKey(source_ref))]),
                },
            ],
            warnings: smallvec![],
        };
        let target_master_plugins = FxHashSet::default();
        let mut state = FullPluginRunState::default();

        state.capture_record_refs(&record, source_plugin, &target_master_plugins);

        assert_eq!(state.unresolved_ref_count(), 1);
        assert_eq!(
            state.unresolved_source_ref_owners.get(&source_ref).unwrap(),
            &vec![RefOwner { owner, owner_sig }]
        );
    }

    #[test]
    fn capture_raw_zero_master_refs_records_placed_ref_payloads() {
        let interner = StringInterner::new();
        let output_plugin = interner.intern("Output.esm");
        let target_master_plugin = interner.intern("Fallout4.esm");
        let owner_sig = SigCode::from_str("ACHR").unwrap();
        let owner = FormKey {
            local: 0x2E803A,
            plugin: output_plugin,
        };
        let location = FormKey {
            local: 0x2E8048,
            plugin: target_master_plugin,
        };
        let record = Record {
            sig: owner_sig,
            form_key: owner,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec![
                FieldEntry {
                    sig: SubrecordSig::from_str("XLCN").unwrap(),
                    value: FieldValue::Bytes(smallvec::SmallVec::from_slice(
                        &0x002E8048_u32.to_le_bytes()
                    )),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("XAPR").unwrap(),
                    value: FieldValue::Bytes(smallvec::SmallVec::from_slice(&[
                        0xF5, 0xB2, 0x50, 0x07, 0, 0, 0, 0
                    ])),
                },
            ],
            warnings: smallvec![],
        };
        let mut state = FullPluginRunState::default();

        state.capture_raw_zero_master_refs(&record, target_master_plugin);

        assert_eq!(
            state.target_master_ref_owners.get(&location).unwrap(),
            &vec![RefOwner { owner, owner_sig }]
        );
        assert!(!state.target_master_ref_owners.contains_key(&FormKey {
            local: 0x50B2F5,
            plugin: target_master_plugin,
        }));
    }

    #[test]
    fn persisted_target_record_view_drops_fields_not_in_target_record_def() {
        let interner = StringInterner::new();
        let output_plugin = interner.intern("Output.esp");
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let record = Record {
            sig: SigCode::from_str("WEAP").unwrap(),
            form_key: FormKey {
                local: 0x800,
                plugin: output_plugin,
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec![
                FieldEntry {
                    sig: SubrecordSig::from_str("EDID").unwrap(),
                    value: FieldValue::String(interner.intern("B21_TestWeapon")),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("ZZZZ").unwrap(),
                    value: FieldValue::Uint(1),
                },
            ],
            warnings: smallvec![],
        };

        let view = persisted_target_record_view(
            &record,
            &schema,
            &interner,
            "Output.esp",
            &FxHashSet::default(),
        );

        assert_eq!(view.fields.len(), 1);
        assert_eq!(view.fields[0].sig, SubrecordSig::from_str("EDID").unwrap());
    }

    #[test]
    fn target_schema_record_view_preserves_source_formkeys_for_capture() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("Source.esp");
        let output_plugin = interner.intern("Output.esp");
        let source_ref = FormKey {
            local: 0x800,
            plugin: source_plugin,
        };
        let owner = FormKey {
            local: 0x801,
            plugin: output_plugin,
        };
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let record = Record {
            sig: SigCode::from_str("WEAP").unwrap(),
            form_key: owner,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec![
                FieldEntry {
                    sig: SubrecordSig::from_str("EDID").unwrap(),
                    value: FieldValue::List(vec![FieldValue::FormKey(source_ref)]),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("ZZZZ").unwrap(),
                    value: FieldValue::FormKey(source_ref),
                },
            ],
            warnings: smallvec![],
        };

        let view = target_schema_record_view(&record, &schema);
        let mut state = FullPluginRunState::default();
        state.capture_record_refs(&view, source_plugin, &FxHashSet::default());

        assert_eq!(view.fields.len(), 1);
        assert_eq!(
            state.unresolved_source_ref_owners.get(&source_ref).unwrap(),
            &vec![RefOwner {
                owner,
                owner_sig: SigCode::from_str("WEAP").unwrap(),
            }]
        );
    }

    #[test]
    fn persisted_target_record_view_normalizes_non_master_formkeys() {
        let interner = StringInterner::new();
        let output_plugin = interner.intern("Output.esp");
        let missing_plugin = interner.intern("Missing.esp");
        let master_plugin = interner.intern("Fallout4.esm");
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let missing_ref = FormKey {
            local: 0x800,
            plugin: missing_plugin,
        };
        let master_ref = FormKey {
            local: 0x801,
            plugin: master_plugin,
        };
        let zero_ref = FormKey {
            local: 0,
            plugin: missing_plugin,
        };
        let mut target_master_plugins = FxHashSet::default();
        target_master_plugins.insert(master_plugin);
        let record = Record {
            sig: SigCode::from_str("WEAP").unwrap(),
            form_key: FormKey {
                local: 0x802,
                plugin: output_plugin,
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec![FieldEntry {
                sig: SubrecordSig::from_str("EDID").unwrap(),
                value: FieldValue::List(vec![
                    FieldValue::FormKey(missing_ref),
                    FieldValue::FormKey(master_ref),
                    FieldValue::FormKey(zero_ref),
                ]),
            }],
            warnings: smallvec![],
        };

        let view = persisted_target_record_view(
            &record,
            &schema,
            &interner,
            "Output.esp",
            &target_master_plugins,
        );

        let FieldValue::List(values) = &view.fields[0].value else {
            panic!("expected list");
        };
        assert_eq!(
            values[0],
            FieldValue::FormKey(FormKey {
                local: missing_ref.local,
                plugin: output_plugin,
            })
        );
        assert_eq!(values[1], FieldValue::FormKey(master_ref));
        assert_eq!(values[2], FieldValue::FormKey(zero_ref));
    }
}
