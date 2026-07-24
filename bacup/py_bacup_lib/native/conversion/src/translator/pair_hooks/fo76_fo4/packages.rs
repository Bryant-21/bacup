use super::*;

pub(super) const PACK_PROCEDURE_TREE_BOUNDARY_SIGS: &[[u8; 4]] =
    &[*b"UNAM", *b"BNAM", *b"POBA", *b"POEA", *b"POCA"];
impl Fo76Fo4Hook {
    pub(super) fn map_fo76_fallback_package_procedure(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"PACK" {
            return;
        }

        let mut after_package_data_marker = false;
        for entry in &mut record.fields {
            if entry.sig.0 == *b"XNAM" {
                after_package_data_marker = true;
                continue;
            }
            if !after_package_data_marker || !matches!(&entry.sig.0, b"ANAM" | b"PNAM") {
                continue;
            }
            match &mut entry.value {
                FieldValue::Bytes(bytes) => {
                    let value = bytes
                        .as_slice()
                        .strip_suffix(&[0])
                        .unwrap_or(bytes.as_slice());
                    if value == b"Fallback" {
                        bytes.clear();
                        bytes.extend_from_slice(b"Sequence\0");
                    }
                }
                FieldValue::String(sym) if interner.resolve(*sym) == Some("Fallback") => {
                    *sym = interner.intern("Sequence");
                }
                _ => {}
            }
        }
    }

    pub(super) fn normalize_fo76_pack_procedure_tree(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"PACK" {
            return;
        }

        let Some(tree_start) = record
            .fields
            .iter()
            .position(|entry| entry.sig.0 == *b"XNAM")
            .map(|index| index + 1)
        else {
            return;
        };
        let tree_end = record.fields[tree_start..]
            .iter()
            .position(|entry| PACK_PROCEDURE_TREE_BOUNDARY_SIGS.contains(&entry.sig.0))
            .map(|offset| tree_start + offset)
            .unwrap_or(record.fields.len());
        if tree_start >= tree_end {
            return;
        }

        let mut rebuilt = Vec::with_capacity(record.fields.len());
        rebuilt.extend(record.fields[..tree_start].iter().cloned());
        rebuilt.extend(Self::rewrite_pack_procedure_tree_entries(
            interner,
            &record.fields[tree_start..tree_end],
        ));
        rebuilt.extend(record.fields[tree_end..].iter().cloned());
        record.fields = smallvec::SmallVec::from_vec(rebuilt);
    }

    pub(super) fn rewrite_pack_procedure_tree_entries(
        interner: &crate::sym::StringInterner,
        entries: &[FieldEntry],
    ) -> Vec<FieldEntry> {
        let mut out = Vec::with_capacity(entries.len());
        let mut carry_root: Vec<FieldEntry> = Vec::new();
        let mut group_start = 0usize;
        while group_start < entries.len() {
            if entries[group_start].sig.0 != *b"ANAM" {
                out.push(entries[group_start].clone());
                group_start += 1;
                continue;
            }

            let group_end = entries[group_start + 1..]
                .iter()
                .position(|entry| entry.sig.0 == *b"ANAM")
                .map(|offset| group_start + 1 + offset)
                .unwrap_or(entries.len());
            let group = &entries[group_start..group_end];
            group_start = group_end;

            let Some(branch_type) = Self::pack_tree_entry_text(interner, &group[0]) else {
                out.extend(group.iter().cloned());
                continue;
            };
            let is_procedure = branch_type == "Procedure";
            let mut branch_entry = group[0].clone();
            Self::map_pack_tree_branch_type_value(interner, &mut branch_entry);

            let mut prefix = Vec::new();
            let mut root = Vec::new();
            let mut procedure = Vec::new();
            let mut trailing = Vec::new();
            let mut procedure_started = false;
            let mut ignored_numeric_procedure = false;

            for entry in &group[1..] {
                if entry.sig.0 == *b"PRCB" {
                    root.push(entry.clone());
                    continue;
                }

                if entry.sig.0 == *b"PNAM" {
                    if let Some(mapped) = Self::fo76_pack_procedure_name(interner, entry) {
                        let mut mapped_entry = entry.clone();
                        Self::set_pack_tree_text_value(interner, &mut mapped_entry, mapped);
                        procedure.push(mapped_entry);
                        procedure_started = true;
                    } else {
                        ignored_numeric_procedure = true;
                    }
                    continue;
                }

                if procedure_started
                    && matches!(&entry.sig.0, b"FNAM" | b"PKC2" | b"PFO2" | b"PFOR")
                {
                    procedure.push(entry.clone());
                } else if ignored_numeric_procedure && entry.sig.0 == *b"PKC2" {
                    trailing.push(entry.clone());
                } else {
                    prefix.push(entry.clone());
                }
            }

            if is_procedure {
                if procedure.is_empty() {
                    out.extend(trailing);
                    if !root.is_empty() {
                        carry_root.extend(root);
                    }
                    continue;
                }
                out.push(branch_entry);
                out.extend(prefix);
                out.extend(procedure);
                out.extend(trailing);
                if !root.is_empty() {
                    carry_root.extend(root);
                }
            } else {
                out.push(branch_entry);
                out.extend(prefix);
                if !carry_root.is_empty() {
                    out.append(&mut carry_root);
                } else {
                    out.extend(root);
                }
                if !procedure.is_empty() {
                    out.push(Self::pack_tree_text_entry(interner, "ANAM", "Procedure"));
                    out.push(Self::pack_tree_u32_entry("CITC", 0));
                    out.extend(procedure);
                }
                out.extend(trailing);
            }
        }

        out.extend(carry_root);
        out
    }

    pub(super) fn map_pack_tree_branch_type_value(
        interner: &crate::sym::StringInterner,
        entry: &mut FieldEntry,
    ) {
        let Some(value) = Self::pack_tree_entry_text(interner, entry) else {
            return;
        };
        if value == "Fallback" {
            Self::set_pack_tree_text_value(interner, entry, "Sequence");
        }
    }

    pub(super) fn pack_tree_entry_text(
        interner: &crate::sym::StringInterner,
        entry: &FieldEntry,
    ) -> Option<String> {
        match &entry.value {
            FieldValue::Bytes(bytes) => {
                let value = trim_nul_suffix(bytes.as_slice());
                std::str::from_utf8(value).ok().map(str::to_owned)
            }
            FieldValue::String(sym) => interner.resolve(*sym).map(str::to_owned),
            _ => None,
        }
    }

    pub(super) fn set_pack_tree_text_value(
        interner: &crate::sym::StringInterner,
        entry: &mut FieldEntry,
        value: &str,
    ) {
        entry.value = FieldValue::String(interner.intern(value));
    }

    pub(super) fn pack_tree_text_entry(
        interner: &crate::sym::StringInterner,
        sig: &str,
        value: &str,
    ) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).expect("valid PACK procedure tree sig"),
            value: FieldValue::String(interner.intern(value)),
        }
    }

    pub(super) fn pack_tree_u32_entry(sig: &str, value: u32) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).expect("valid PACK procedure tree sig"),
            value: FieldValue::Uint(value as u64),
        }
    }

    pub(super) fn fo76_pack_procedure_name(
        interner: &crate::sym::StringInterner,
        entry: &FieldEntry,
    ) -> Option<&'static str> {
        let value = match &entry.value {
            FieldValue::Bytes(bytes) => trim_nul_suffix(bytes.as_slice()).to_vec(),
            FieldValue::String(sym) => interner.resolve(*sym)?.as_bytes().to_vec(),
            _ => return None,
        };
        match value.as_slice() {
            b"Trav" | b"Travel" => Some("Travel"),
            b"Sand" | b"Sandbox" => Some("Sandbox"),
            b"Foll" | b"Follow" => Some("Follow"),
            b"Wait" => Some("Wait"),
            b"Patr" | b"Patrol" => Some("Patrol"),
            b"Sit" => Some("Sit"),
            b"UseW" | b"UseWeapon" => Some("UseWeapon"),
            b"Rang" | b"Range" => Some("Range"),
            b"Unlo" | b"UnlockDoors" => Some("UnlockDoors"),
            b"Acti" | b"Activate" => Some("Activate"),
            b"Find" => Some("Find"),
            b"Esco" | b"Escort" => Some("Escort"),
            b"Hold" | b"HoldPosition" => Some("HoldPosition"),
            b"Slee" | b"Sleep" => Some("Sleep"),
            b"Guar" | b"Guard" => Some("Guard"),
            b"Eat" => Some("Eat"),
            b"Say" | b"ForceGreet" => Some("ForceGreet"),
            b"Flee" => Some("Flee"),
            b"Head" | b"Headtrack" => Some("Headtrack"),
            b"Orbi" | b"Orbit" => Some("Orbit"),
            b"UseI" | b"UseIdleMarker" => Some("UseIdleMarker"),
            // Procedures present in FO76 with an identical FO4 procedure name
            // (verified against vanilla Fallout4.esm). Without these mappings the
            // procedure-tree rewrite drops the item, producing CK "missing
            // procedure / missing procedure tree item" on the owning package.
            b"GuardArea" => Some("GuardArea"),
            b"Hover" => Some("Hover"),
            b"KeepAnEyeOn" => Some("KeepAnEyeOn"),
            b"LockDoors" => Some("LockDoors"),
            b"UseMagic" => Some("UseMagic"),
            b"Acquire" => Some("Acquire"),
            b"FollowTo" => Some("FollowTo"),
            _ => None,
        }
    }
}
