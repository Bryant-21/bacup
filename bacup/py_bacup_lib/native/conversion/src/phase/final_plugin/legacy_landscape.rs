use std::collections::HashSet;
use std::path::{Path, PathBuf};

use materials_native::texture_convert::legacy_specular_exponent_to_fo4_smoothness;

use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::phase::{LogLevel, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::session::{PluginSession, open_session};
use crate::sym::StringInterner;
use crate::translator::Game;

struct LandscapeSpecJob {
    land: Record,
    texture: Record,
    input: PathBuf,
    output: PathBuf,
    exponent: u8,
}

fn field<'a>(record: &'a Record, sig: &[u8; 4]) -> Option<&'a FieldValue> {
    record
        .fields
        .iter()
        .find(|field| &field.sig.0 == sig)
        .map(|field| &field.value)
}

fn texture_path(record: &Record, sig: &[u8; 4], interner: &StringInterner) -> Option<String> {
    match field(record, sig)? {
        FieldValue::String(sym) => Some(interner.resolve(*sym)?.to_owned()),
        _ => None,
    }
}

fn prepare_jobs(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    schema: &AuthoringSchema,
    data_root: &Path,
) -> Result<Vec<LandscapeSpecJob>, String> {
    let mut keys = session
        .form_keys_of_sig(SigCode(*b"LTEX"), mapper.interner)
        .map_err(|e| e.to_string())?;
    keys.sort_by_key(|key| key.local);
    let mut jobs = Vec::new();
    for key in keys {
        let mut land = session
            .record_decoded(&key, schema, mapper.interner)
            .map_err(|e| e.to_string())?;
        let Some(FieldValue::FormKey(texture_key)) = field(&land, b"TNAM") else {
            continue;
        };
        if texture_key.plugin != mapper.output_plugin_sym() {
            continue;
        }
        let exponent = match field(&land, b"SNAM") {
            Some(FieldValue::Uint(n)) => u8::try_from(*n).ok(),
            Some(FieldValue::Int(n)) => u8::try_from(*n).ok(),
            Some(FieldValue::Bytes(data)) if data.len() == 1 => Some(data[0]),
            _ => None,
        }
        .ok_or_else(|| format!("LTEX {:06X}: missing source specular exponent", key.local))?;
        let mut texture = session
            .record_decoded(texture_key, schema, mapper.interner)
            .map_err(|e| e.to_string())?;
        let spec = texture_path(&texture, b"TX07", mapper.interner).ok_or_else(|| {
            format!(
                "LTEX {:06X}: TXST has no converted smooth/spec map",
                key.local
            )
        })?;
        let spec = crate::relocation::normalize_rel(&spec);
        let spec = spec.strip_prefix("textures/").unwrap_or(&spec);
        if spec.contains(':') || spec.split('/').any(|part| part == "..") {
            return Err(format!("invalid landscape specular path {spec}"));
        }
        let suffix = format!("_bacup_land{exponent}.dds");
        if spec.ends_with(&suffix) {
            let existing = data_root.join("textures").join(spec);
            directxtex_native::read_dds_mips_rgba8(&existing)
                .map_err(|e| format!("landscape smooth/spec {}: {e}", existing.display()))?;
            continue;
        }
        let stem = spec
            .strip_suffix(".dds")
            .ok_or_else(|| format!("invalid landscape specular path {spec}"))?;
        let output = format!("{stem}{suffix}");
        let texture_key = mapper.allocate_or_resolve(
            FormKey {
                plugin: mapper.interner.intern("__legacy_landscape_spec__"),
                local: key.local,
            },
            None,
            SigCode(*b"TXST"),
        );
        texture.form_key = texture_key;
        texture.eid = None;
        texture
            .fields
            .retain(|field| !matches!(&field.sig.0, b"EDID" | b"TX07"));
        texture.fields.push(FieldEntry {
            sig: SubrecordSig(*b"TX07"),
            value: FieldValue::String(mapper.interner.intern(&output)),
        });
        for field in &mut land.fields {
            if field.sig.0 == *b"TNAM" {
                field.value = FieldValue::FormKey(texture_key);
            }
        }
        jobs.push(LandscapeSpecJob {
            land,
            texture,
            input: data_root.join("textures").join(spec),
            output: data_root.join("textures").join(output),
            exponent,
        });
    }
    Ok(jobs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase::Phase;
    use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_load_no_py,
        plugin_handle_new_no_py, plugin_handle_save_no_py,
    };
    use std::sync::{Arc, atomic::AtomicBool};

    fn run_final(handle: u64, root: &Path) -> Result<PhaseReport, RunError> {
        let id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: 0,
            target_handle_id: handle,
            master_handle_ids: vec![],
            config: RunConfig::default(),
        })?;
        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            assert!(run.mapper_state.is_none());
            let params = serde_json::json!({"regenerate_modt": false, "rebuild_cell_offsets": false,
                "convert_legacy_landscape_specular": true, "fail_on_duplicate_form_ids": true});
            let cancel = Arc::new(AtomicBool::new(false));
            let mut ctx = PhaseCtx {
                run,
                mod_path: root,
                source_extracted_dir: root,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            super::super::FinalizePluginRecordsPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        });
        drop_run(id).unwrap();
        report
    }

    #[test]
    fn legacy_landscape_specular_preserves_shared_mesh_and_distinct_land_exponents_after_save() {
        const OUTPUT: &str = "LandscapeShine.esp";
        let directory = tempfile::tempdir().unwrap();
        let data_root = directory.path().join("data");
        let texture_dir = data_root.join("textures/landscape");
        std::fs::create_dir_all(&texture_dir).unwrap();
        let generic = texture_dir.join("asphalt02_s.dds");
        let chain = [(4, 56_u8), (2, 72), (1, 88)]
            .into_iter()
            .map(|(size, red)| {
                (
                    size,
                    size,
                    [red, 255, 0, 255].repeat((size * size) as usize),
                )
            })
            .collect();
        let original = directxtex_native::encode_dds_from_owned_rgba8_chain(
            chain,
            "R8G8B8A8_UNORM",
            false,
            None,
        )
        .unwrap();
        std::fs::write(&generic, &original).unwrap();
        let interner = StringInterner::new();
        let key = |local| FormKey {
            plugin: interner.intern(OUTPUT),
            local,
        };
        let mut texture = Record::new(SigCode(*b"TXST"), key(0x900));
        for (sig, path) in [
            (*b"TX00", "Landscape/Asphalt02.dds"),
            (*b"TX01", "Landscape/Asphalt02_n.dds"),
        ] {
            texture.fields.push(FieldEntry {
                sig: SubrecordSig(sig),
                value: FieldValue::String(interner.intern(path)),
            });
        }
        texture.fields.push(FieldEntry {
            sig: SubrecordSig(*b"TX07"),
            value: FieldValue::String(interner.intern("Landscape/Asphalt02_s.dds")),
        });
        let mut records = vec![texture];
        for (local, exponent) in [(0xA00, 30_u64), (0xA01, 60)] {
            let mut land = Record::new(SigCode(*b"LTEX"), key(local));
            land.fields.extend([
                FieldEntry {
                    sig: SubrecordSig(*b"TNAM"),
                    value: FieldValue::FormKey(key(0x900)),
                },
                FieldEntry {
                    sig: SubrecordSig(*b"SNAM"),
                    value: FieldValue::Uint(exponent),
                },
            ]);
            records.push(land);
        }
        let handle = plugin_handle_new_no_py(OUTPUT, Some("fo4"));
        plugin_handle_add_master_native(handle, "Fallout4.esm", None).unwrap();
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        {
            let mut session = open_session(handle, None).unwrap();
            session.add_records(records, &schema, &interner).unwrap();
            session.target_slot_mut().parsed.header.next_object_id = 0x900;
        }
        let path = directory.path().join(OUTPUT);
        plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
        assert!(plugin_handle_close_native(handle));
        let handle =
            plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, false)
                .unwrap();
        {
            let mut session = open_session(handle, None).unwrap();
            let saved = session
                .record_decoded(&key(0x900), &schema, &interner)
                .unwrap();
            assert!(
                texture_path(&saved, b"TX07", &interner).is_some(),
                "decoded TXST: {saved:?}"
            );
        }
        let report = run_final(handle, directory.path()).unwrap();
        assert_eq!(
            (
                report.assets_written,
                report.records_changed,
                report.records_added
            ),
            (2, 2, 2)
        );
        for source_exponent in [30_u8, 60] {
            let output = texture_dir.join(format!("asphalt02_s_bacup_land{source_exponent}.dds"));
            let converted = directxtex_native::read_dds_mips_rgba8(&output).unwrap();
            assert_eq!(converted.mips.len(), 3);
            for ((_, _, mip), red) in converted.mips.iter().zip([56_i32, 72, 88]) {
                for pixel in mip.chunks_exact(4) {
                    assert!((i32::from(pixel[0]) - red).abs() <= 1);
                    let exponent = (10.0 * f32::from(pixel[1]) / 255.0 + 1.0).exp2();
                    assert!((exponent / f32::from(source_exponent) - 1.0).abs() < 0.03);
                }
            }
        }
        assert_eq!(std::fs::read(&generic).unwrap(), original);
        plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
        assert!(plugin_handle_close_native(handle));
        let handle =
            plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true)
                .unwrap();
        {
            let mut session = open_session(handle, None).unwrap();
            let original = session
                .record_decoded(&key(0x900), &schema, &interner)
                .unwrap();
            assert_eq!(
                texture_path(&original, b"TX07", &interner).as_deref(),
                Some("Landscape/Asphalt02_s.dds")
            );
            for (local, suffix) in [(0xA00, "_bacup_land30.dds"), (0xA01, "_bacup_land60.dds")] {
                let land = session
                    .record_decoded(&key(local), &schema, &interner)
                    .unwrap();
                let Some(FieldValue::FormKey(key)) = field(&land, b"TNAM") else {
                    panic!("land texture reference")
                };
                let texture = session.record_decoded(key, &schema, &interner).unwrap();
                assert!(
                    texture_path(&texture, b"TX07", &interner)
                        .unwrap()
                        .ends_with(suffix)
                );
            }
        }
        let repeated = run_final(handle, directory.path()).unwrap();
        assert_eq!(
            (
                repeated.assets_written,
                repeated.records_changed,
                repeated.records_added
            ),
            (0, 0, 0)
        );
        let variant = texture_dir.join("asphalt02_s_bacup_land30.dds");
        std::fs::remove_file(&variant).unwrap();
        let missing = run_final(handle, directory.path()).unwrap_err();
        assert!(missing.to_string().contains("asphalt02_s_bacup_land30.dds"));
        std::fs::write(&variant, b"invalid DDS").unwrap();
        let corrupt = run_final(handle, directory.path()).unwrap_err();
        assert!(corrupt.to_string().contains("asphalt02_s_bacup_land30.dds"));
        assert!(plugin_handle_close_native(handle));
    }
}

fn write_specular(input: &Path, output: &Path, exponent: u8) -> Result<(), String> {
    let mut image = directxtex_native::read_dds_mips_rgba8(input)
        .map_err(|e| format!("landscape smooth/spec {}: {e}", input.display()))?;
    let smoothness =
        (legacy_specular_exponent_to_fo4_smoothness(f32::from(exponent)) * 255.0).round() as u8;
    for (_, _, pixels) in &mut image.mips {
        for pixel in pixels.chunks_exact_mut(4) {
            pixel[1] = smoothness;
        }
    }
    let bytes =
        directxtex_native::encode_dds_from_owned_rgba8_chain(image.mips, "BC5_UNORM", false, None)?;
    std::fs::write(output, bytes).map_err(|e| format!("write {}: {e}", output.display()))
}

pub(super) fn convert(ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
    if ctx.run.target != Game::Fo4
        || !matches!(ctx.run.source, Game::Fnv | Game::Fo3 | Game::SkyrimSe)
    {
        return Ok(PhaseReport::default());
    }
    let jobs = {
        let mut session = open_session(ctx.run.target_handle_id, None)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        if session
            .form_keys_of_sig(SigCode(*b"LTEX"), &ctx.run.interner)
            .map_err(|e| PhaseError::Internal(e.to_string()))?
            .is_empty()
        {
            return Ok(PhaseReport::default());
        }
        let mut mapper = FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: session.target_slot().parsed.plugin_name.to_string(),
                generated_object_id_floor: session
                    .target_slot()
                    .parsed
                    .header
                    .next_object_id
                    .max(0x800),
                ..Default::default()
            },
            &ctx.run.interner,
        );
        mapper.reserve_object_ids(
            session
                .own_record_signatures_in_handle(ctx.run.target_handle_id)
                .map_err(|e| PhaseError::Internal(e.to_string()))?
                .into_keys(),
        );
        prepare_jobs(
            &mut session,
            &mut mapper,
            &ctx.run.schema_target,
            &ctx.mod_path.join("data"),
        )
        .map_err(PhaseError::Internal)?
    };
    let mut written = HashSet::new();
    for job in &jobs {
        ctx.check_cancel()?;
        if written.insert(job.output.clone()) {
            write_specular(&job.input, &job.output, job.exponent).map_err(PhaseError::Internal)?;
        }
    }
    let mut session = open_session(ctx.run.target_handle_id, None)
        .map_err(|e| PhaseError::Internal(e.to_string()))?;
    let changed = jobs.len() as u32;
    for job in jobs {
        session
            .add_records(vec![job.texture], &ctx.run.schema_target, &ctx.run.interner)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        session
            .replace_record_contents(job.land, &ctx.run.schema_target, &ctx.run.interner)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
    }
    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
        phase: "finalize_plugin_records",
        level: LogLevel::Info,
        message: format!(
            "finalize_plugin_records: {} landscape smooth/spec maps retain source highlight exponents",
            written.len()
        ),
    });
    Ok(PhaseReport {
        assets_written: written.len() as u32,
        records_added: changed,
        records_changed: changed,
        ..Default::default()
    })
}
