use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue, Record};
use crate::translator::pair_hook::{HookError, HookResult, PairCtx};

pub(super) fn normalize_skyrim_ammo(record: &mut Record, ctx: &PairCtx<'_>) -> HookResult {
    if record.sig.0 != *b"AMMO" || record.fields.iter().any(|field| field.sig.0 == *b"DNAM") {
        return Ok(());
    }
    let Some(data_index) = record
        .fields
        .iter()
        .position(|field| field.sig.0 == *b"DATA")
    else {
        return Ok(());
    };
    let FieldValue::Bytes(source) = &record.fields[data_index].value else {
        return Err(HookError::Runtime(
            "Skyrim AMMO requires source DATA bytes".into(),
        ));
    };
    if !matches!(source.len(), 16 | 20) {
        return Err(HookError::Runtime(format!(
            "Skyrim AMMO has unsupported DATA length {}",
            source.len()
        )));
    }
    let raw_projectile = u32::from_le_bytes(source[..4].try_into().unwrap());
    let projectile = if raw_projectile == 0 {
        FieldValue::Uint(0)
    } else {
        FieldValue::FormKey(ctx.resolve_source_form_id(raw_projectile).ok_or_else(|| {
            HookError::Runtime(format!(
                "Skyrim AMMO projectile {raw_projectile:08X} has no source owner"
            ))
        })?)
    };
    let flags = u32::from_le_bytes(source[4..8].try_into().unwrap());
    let damage = f32::from_le_bytes(source[8..12].try_into().unwrap());
    let value = u32::from_le_bytes(source[12..16].try_into().unwrap());
    let weight = if source.len() == 20 {
        f32::from_le_bytes(source[16..20].try_into().unwrap())
    } else {
        0.0
    };
    record.fields[data_index].value = FieldValue::Struct(vec![
        (ctx.interner.intern("value"), FieldValue::Uint(value.into())),
        (ctx.interner.intern("weight"), FieldValue::Float(weight)),
    ]);
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"DNAM"),
        value: FieldValue::Struct(vec![
            (ctx.interner.intern("projectile"), projectile),
            // Skyrim's NonBolt bit shares FO4's unrelated HasCountBased3D position.
            (
                ctx.interner.intern("flags"),
                FieldValue::Uint((flags & 3).into()),
            ),
            (ctx.interner.intern("unknown_u8_2"), FieldValue::Uint(0)),
            (ctx.interner.intern("unknown_u8_3"), FieldValue::Uint(0)),
            (ctx.interner.intern("unknown_u8_4"), FieldValue::Uint(0)),
            (ctx.interner.intern("damage"), FieldValue::Float(damage)),
            (ctx.interner.intern("health"), FieldValue::Uint(0)),
        ]),
    });
    Ok(())
}
