use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;

pub(crate) mod actor_stats;
mod output;

pub(crate) use output::prepare_magic_runtime_output;

const SOURCE_NAMESPACE: &str = "B21_SkyrimMagicRuntime.esm";
const RESOURCES: [(u32, &str, f32, u32, u32); 7] = [
    (25, "Magicka", 0.0, 0x0A002000, 9),
    (28, "MagickaRate", 0.0, 0x2000, 8),
    (156, "MagickaRateMult", 100.0, 0x2000, 8),
    (u32::MAX, "MagickaRegenDelay", 0.0, 0x2000, 8),
    (u32::MAX, "ActorAttributeLevel", 0.0, 0x2000, 8),
    (u32::MAX, "StaminaRegenDelay", 0.0, 0x2000, 8),
    (45, "ResistDisease", 0.0, 0x2000, 4),
];

pub(crate) fn magic_resource_source_key(
    value: &FieldValue,
    interner: &StringInterner,
) -> Option<FormKey> {
    RESOURCES
        .iter()
        .enumerate()
        .filter(|(_, (number, ..))| *number != u32::MAX)
        .find_map(|(index, (number, name, ..))| {
            let matches = match value {
                FieldValue::Uint(value) => *value == u64::from(*number),
                FieldValue::Int(value) => *value == i64::from(*number),
                FieldValue::String(value) => interner.resolve(*value).is_some_and(|value| {
                    value.eq_ignore_ascii_case(name) || value.parse::<u32>() == Ok(*number)
                }),
                _ => false,
            };
            matches.then(|| source_key(index, interner))
        })
}

fn source_key(index: usize, interner: &StringInterner) -> FormKey {
    FormKey {
        local: index as u32 + 1,
        plugin: interner.intern(SOURCE_NAMESPACE),
    }
}

pub(crate) fn allocate_magic_resource_records(
    mapper: &mut FormKeyMapper<'_>,
    previous: Option<[FormKey; 7]>,
    interner: &StringInterner,
) -> ([FormKey; 7], Vec<Record>) {
    let targets = previous.unwrap_or_else(|| std::array::from_fn(|_| mapper.allocate_generated()));
    for (index, target) in targets.iter().copied().enumerate() {
        mapper.add_mapping(source_key(index, interner), target);
    }
    let records = if previous.is_some() {
        Vec::new()
    } else {
        RESOURCES
            .iter()
            .enumerate()
            .map(|(index, (_, name, default, flags, kind))| {
                let mut record = Record::new(SigCode(*b"AVIF"), targets[index]);
                record.eid = Some(interner.intern(&format!("B21_Skyrim{name}")));
                record.fields.extend([
                    field(*b"NAM0", FieldValue::Float(*default)),
                    field(*b"AVFL", FieldValue::Uint((*flags).into())),
                    field(*b"NAM1", FieldValue::Uint((*kind).into())),
                ]);
                record
            })
            .collect()
    };
    (targets, records)
}

fn field(sig: [u8; 4], value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig(sig),
        value,
    }
}

#[cfg(test)]
mod tests;
