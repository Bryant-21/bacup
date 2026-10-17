use std::collections::HashSet;

use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;
use crate::translator::Game;

use super::{insert_namespace_after_root, normalize_texture_member};

fn texture_key(value: &FieldValue, interner: &StringInterner) -> Option<String> {
    let path = match value {
        FieldValue::String(sym) => interner.resolve(*sym)?.to_owned(),
        FieldValue::Bytes(bytes) => std::str::from_utf8(bytes).ok()?.to_owned(),
        _ => return None,
    };
    Some(normalize_texture_member(&path))
}

pub(crate) fn namespace_legacy_texture_set_paths(
    record: &mut Record,
    members: &HashSet<String>,
    namespace: &str,
    source: Game,
    target: Game,
    interner: &StringInterner,
) -> usize {
    if record.sig.0 != *b"TXST"
        || target != Game::Fo4
        || !matches!(source, Game::Fnv | Game::Fo3 | Game::SkyrimSe)
        || members.is_empty()
        || namespace.is_empty()
    {
        return 0;
    }
    // LegacySpecGloss emits a new _s beside its normal; that generated file
    // cannot itself occur in the source relocation inventory.
    let relocated_spec = record.fields.iter().find_map(|field| {
        if field.sig.0 != *b"TX01" {
            return None;
        }
        let key = texture_key(&field.value, interner)?;
        members.contains(&key).then_some(())?;
        Some(format!("{}_s.dds", key.strip_suffix("_n.dds")?))
    });
    let mut updated = 0;
    for field in &mut record.fields {
        if !matches!(
            &field.sig.0,
            b"TX00" | b"TX01" | b"TX02" | b"TX03" | b"TX04" | b"TX05" | b"TX06" | b"TX07"
        ) {
            continue;
        }
        let Some(key) = texture_key(&field.value, interner) else {
            continue;
        };
        if !members.contains(&key)
            && !(field.sig.0 == *b"TX07" && relocated_spec.as_ref() == Some(&key))
        {
            continue;
        }
        let path = insert_namespace_after_root(&key, namespace);
        let path = path.strip_prefix("textures/").unwrap_or(&path);
        match &mut field.value {
            FieldValue::String(sym) => *sym = interner.intern(path),
            FieldValue::Bytes(bytes) => {
                let terminated = bytes.last() == Some(&0);
                bytes.clear();
                bytes.extend_from_slice(path.as_bytes());
                if terminated {
                    bytes.push(0);
                }
            }
            _ => unreachable!(),
        }
        updated += 1;
    }
    updated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    fn texture_set(interner: &StringInterner, specular: &str) -> Record {
        let mut record = Record::new(
            SigCode(*b"TXST"),
            FormKey::parse("0139A0@Skyrim.esm", interner).unwrap(),
        );
        for (sig, path) in [
            (*b"TX00", "Landscape/Dirt02.dds"),
            (*b"TX01", "Data\\Textures\\Landscape\\Dirt02_n.DDS"),
            (*b"TX04", "Shared/Cube.dds"),
            (*b"TX07", specular),
        ] {
            record.fields.push(FieldEntry {
                sig: SubrecordSig(sig),
                value: FieldValue::String(interner.intern(path)),
            });
        }
        record
    }

    #[test]
    fn legacy_texture_set_relocation_tracks_generated_spec_without_moving_shared_slots() {
        let interner = StringInterner::new();
        let members = ["textures/landscape/dirt02_n.dds".to_string()]
            .into_iter()
            .collect();
        for source in [Game::Fnv, Game::Fo3, Game::SkyrimSe] {
            let mut record = texture_set(&interner, "Landscape/Dirt02_s.dds");
            assert_eq!(
                namespace_legacy_texture_set_paths(
                    &mut record,
                    &members,
                    "Skyrim",
                    source,
                    Game::Fo4,
                    &interner
                ),
                2
            );
            let paths: Vec<_> = record
                .fields
                .iter()
                .map(|field| texture_key(&field.value, &interner).unwrap())
                .collect();
            assert_eq!(
                paths,
                [
                    "textures/landscape/dirt02.dds",
                    "textures/skyrim/landscape/dirt02_n.dds",
                    "textures/shared/cube.dds",
                    "textures/skyrim/landscape/dirt02_s.dds"
                ]
            );
            let once = record.fields.clone();
            assert_eq!(
                namespace_legacy_texture_set_paths(
                    &mut record,
                    &members,
                    "Skyrim",
                    source,
                    Game::Fo4,
                    &interner
                ),
                0
            );
            assert_eq!(record.fields, once);
        }
    }

    #[test]
    fn legacy_texture_set_relocation_preserves_independent_spec_and_raw_termination() {
        let interner = StringInterner::new();
        let members = ["textures/landscape/dirt02_n.dds".to_string()]
            .into_iter()
            .collect();
        let mut record = texture_set(&interner, "Shared/Other_s.dds");
        record.fields[1].value = FieldValue::Bytes(b"Landscape/Dirt02_n.dds\0".as_slice().into());
        assert_eq!(
            namespace_legacy_texture_set_paths(
                &mut record,
                &members,
                "Skyrim",
                Game::SkyrimSe,
                Game::Fo4,
                &interner
            ),
            1
        );
        assert_eq!(
            record.fields[1].value,
            FieldValue::Bytes(b"Skyrim/landscape/dirt02_n.dds\0".as_slice().into())
        );
        assert_eq!(
            texture_key(&record.fields[3].value, &interner).as_deref(),
            Some("textures/shared/other_s.dds")
        );
        for (source, target, sig) in [
            (Game::Fo76, Game::Fo4, *b"TXST"),
            (Game::SkyrimSe, Game::Fnv, *b"TXST"),
            (Game::SkyrimSe, Game::Fo4, *b"MISC"),
        ] {
            let mut record = texture_set(&interner, "Landscape/Dirt02_s.dds");
            record.sig = SigCode(sig);
            let before = record.fields.clone();
            assert_eq!(
                namespace_legacy_texture_set_paths(
                    &mut record,
                    &members,
                    "Skyrim",
                    source,
                    target,
                    &interner
                ),
                0
            );
            assert_eq!(record.fields, before);
        }
    }
}
