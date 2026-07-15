use std::collections::HashSet;

use smallvec::SmallVec;

use crate::ids::SigCode;
use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};
use crate::record::{FieldValue, Record};
use crate::session::open_session;
use crate::sym::StringInterner;
use crate::translator::Game;

pub struct RewriteMswpMaterialPathsPhase;

impl Phase for RewriteMswpMaterialPathsPhase {
    fn name(&self) -> &'static str {
        "rewrite_mswp_material_paths"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        ctx.check_cancel()?;
        if ctx.run.source != Game::Fo76 || ctx.run.target != Game::Fo4 {
            return Ok(PhaseReport::default());
        }
        let namespace = crate::run::base_asset_namespace_for_run(ctx.run);
        if namespace.trim().is_empty() || ctx.run.relocation_members.is_empty() {
            return Ok(PhaseReport::default());
        }

        let mswp_sig =
            SigCode::from_str("MSWP").map_err(|e| PhaseError::Internal(e.to_string()))?;
        let mut session = open_session(ctx.run.target_handle_id, None)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        let fks = session
            .form_keys_of_sig(mswp_sig, &ctx.run.interner)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;

        let mut changed = 0u32;
        for fk in fks {
            ctx.check_cancel()?;
            let mut record = session
                .record_decoded(&fk, &ctx.run.schema_target, &ctx.run.interner)
                .map_err(|e| PhaseError::Internal(e.to_string()))?;
            if rewrite_mswp_relocated_materials(
                &mut record,
                &ctx.run.interner,
                &ctx.run.relocation_members,
                &namespace,
            ) && session
                .replace_record_contents(record, &ctx.run.schema_target, &ctx.run.interner)
                .map_err(|e| PhaseError::Internal(e.to_string()))?
            {
                changed += 1;
            }
        }

        Ok(PhaseReport {
            records_changed: changed,
            ..Default::default()
        })
    }
}

fn rewrite_mswp_relocated_materials(
    record: &mut Record,
    interner: &StringInterner,
    relocation_members: &HashSet<String>,
    namespace: &str,
) -> bool {
    if record.sig.as_str() != "MSWP" {
        return false;
    }

    let mut changed = false;
    for entry in record.fields.iter_mut() {
        if !matches!(entry.sig.as_str(), "BNAM" | "SNAM") {
            continue;
        }
        changed |= rewrite_material_value_if_relocated(
            &mut entry.value,
            interner,
            relocation_members,
            namespace,
        );
    }
    changed
}

fn rewrite_material_value_if_relocated(
    value: &mut FieldValue,
    interner: &StringInterner,
    relocation_members: &HashSet<String>,
    namespace: &str,
) -> bool {
    let Some(current) = field_value_string(value, interner) else {
        return false;
    };
    let Some(key) = material_member_key(&current) else {
        return false;
    };
    if !relocation_member_matches(&key, relocation_members) {
        return false;
    }
    let Some(rewritten) = namespace_material_value(&current, namespace) else {
        return false;
    };
    if rewritten == current {
        return false;
    }

    match value {
        FieldValue::String(sym) => {
            *sym = interner.intern(&rewritten);
        }
        FieldValue::Bytes(bytes) => {
            let mut out = rewritten.into_bytes();
            out.push(0);
            *bytes = SmallVec::from_vec(out);
        }
        _ => return false,
    }
    true
}

fn field_value_string(value: &FieldValue, interner: &StringInterner) -> Option<String> {
    match value {
        FieldValue::String(sym) => interner.resolve(*sym).map(ToOwned::to_owned),
        FieldValue::Bytes(bytes) => std::str::from_utf8(trim_nul_suffix(bytes.as_slice()))
            .ok()
            .map(ToOwned::to_owned),
        _ => None,
    }
}

fn trim_nul_suffix(mut bytes: &[u8]) -> &[u8] {
    while matches!(bytes.last(), Some(0)) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn material_member_key(value: &str) -> Option<String> {
    let raw = value.trim().trim_matches('\0').replace('\\', "/");
    if raw.is_empty() || raw.starts_with("//") {
        return None;
    }

    let mut path = raw.trim_start_matches('/').to_ascii_lowercase();
    if let Some((_, rest)) = path.split_once("/data/") {
        path = rest.to_string();
    } else if let Some(rest) = path.strip_prefix("data/") {
        path = rest.to_string();
    } else if let Some((_, rest)) = path.split_once("/materials/") {
        path = format!("materials/{rest}");
    } else {
        let bytes = path.as_bytes();
        let win_abs = bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'/';
        if win_abs || path.contains(':') {
            return None;
        }
    }

    let rel = path
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/");
    if rel.is_empty() || rel.split('/').any(|part| part == "..") {
        return None;
    }
    let rel = rel.strip_prefix("materials/").unwrap_or(&rel);
    let key = format!("materials/{rel}");
    (key.ends_with(".bgsm") || key.ends_with(".bgem")).then_some(key)
}

fn relocation_member_matches(key: &str, relocation_members: &HashSet<String>) -> bool {
    if !key.contains('*') {
        return relocation_members.contains(key);
    }
    relocation_members
        .iter()
        .any(|member| wildcard_match(key, member))
}

fn wildcard_match(pattern: &str, candidate: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == candidate;
    }

    let parts: Vec<&str> = pattern.split('*').collect();
    let mut remainder = candidate;
    if !pattern.starts_with('*') {
        let first = parts.first().copied().unwrap_or("");
        if !remainder.starts_with(first) {
            return false;
        }
        remainder = &remainder[first.len()..];
    }

    for part in parts
        .iter()
        .copied()
        .filter(|part| !part.is_empty())
        .skip(if pattern.starts_with('*') { 0 } else { 1 })
    {
        let Some(index) = remainder.find(part) else {
            return false;
        };
        remainder = &remainder[index + part.len()..];
    }

    if !pattern.ends_with('*') {
        let last = parts.last().copied().unwrap_or("");
        return candidate.ends_with(last);
    }
    true
}

fn namespace_material_value(value: &str, namespace: &str) -> Option<String> {
    let namespace = namespace.trim().trim_matches(|c| c == '/' || c == '\\');
    if namespace.is_empty() {
        return None;
    }
    let normalized = value.trim().trim_matches('\0').replace('\\', "/");
    if normalized.is_empty() {
        return None;
    }

    let mut parts: Vec<&str> = normalized
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if parts.is_empty() || parts.iter().any(|part| *part == "..") {
        return None;
    }

    let insert_at = if parts
        .first()
        .is_some_and(|part| part.eq_ignore_ascii_case("materials"))
    {
        1
    } else {
        0
    };
    if parts
        .get(insert_at)
        .is_some_and(|part| part.eq_ignore_ascii_case(namespace))
    {
        return None;
    }

    parts.insert(insert_at, namespace);
    Some(parts.join("\\"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SubrecordSig};
    use crate::record::{FieldEntry, Record};

    fn string_field(sig: &str, value: &str, interner: &StringInterner) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::String(interner.intern(value)),
        }
    }

    #[test]
    fn rewrites_relocated_mswp_materials() {
        let interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("MSWP").unwrap(),
            FormKey {
                local: 0x32404a,
                plugin: interner.intern("SeventySix.esm"),
            },
        );
        record.fields.push(string_field(
            "BNAM",
            "Landscape\\Ground\\TEMP_GroundTexture01*.bgsm",
            &interner,
        ));
        record.fields.push(string_field(
            "BNAM",
            "Landscape\\DirtCliffs\\DirtCliffEroded.BGSM",
            &interner,
        ));
        record.fields.push(string_field(
            "SNAM",
            "Landscape\\Ground\\ForestGrass01*.BGSM",
            &interner,
        ));
        record.fields.push(string_field(
            "SNAM",
            "Landscape\\Ground\\ForestDirt01.BGSM",
            &interner,
        ));
        record.fields.push(string_field("SNAM", "", &interner));

        let relocation_members = HashSet::from([
            "materials/landscape/ground/temp_groundtexture01.bgsm".to_string(),
            "materials/landscape/ground/temp_groundtexture01decal.bgsm".to_string(),
            "materials/landscape/ground/forestgrass01.bgsm".to_string(),
            "materials/landscape/ground/forestgrass01decal.bgsm".to_string(),
            "materials/landscape/ground/forestdirt01.bgsm".to_string(),
        ]);

        assert!(rewrite_mswp_relocated_materials(
            &mut record,
            &interner,
            &relocation_members,
            "FO76",
        ));

        let values = record
            .fields
            .iter()
            .map(|entry| match &entry.value {
                FieldValue::String(sym) => interner.resolve(*sym).unwrap().to_string(),
                other => panic!("unexpected value {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            values,
            vec![
                "FO76\\Landscape\\Ground\\TEMP_GroundTexture01*.bgsm",
                "Landscape\\DirtCliffs\\DirtCliffEroded.BGSM",
                "FO76\\Landscape\\Ground\\ForestGrass01*.BGSM",
                "FO76\\Landscape\\Ground\\ForestDirt01.BGSM",
                "",
            ]
        );
    }
}
