//! FO76 face texture sets carry only a BGSM, but FO4 face customization reads
//! the TXST slots and packs them into one texture array with the race's base
//! head. An empty slot, or a slice whose format or size differs from the base
//! head, never finishes loading and the CK spins forever in
//! `BSFaceGenUtils::PrepareHeadPartForShaders` (cell load hangs).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::phase::copy_materialized_facegen::resolve_path_ci;
use crate::phase::{LogLevel, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::open_session;
use crate::sym::StringInterner;
use crate::translator::Game;

const FACEGEN_TEXTURES: u64 = 0x02;
const HUMAN_RACE: u32 = 0x013746;
const GHOUL_RACE: u32 = 0x0EAFB6;
const FACE_SUFFIX: &str = "_bacup_face.dds";

#[derive(Clone, Copy)]
enum Slot {
    Diffuse,
    Normal,
    SmoothSpec,
}

const SLOTS: [Slot; 3] = [Slot::Diffuse, Slot::Normal, Slot::SmoothSpec];

impl Slot {
    fn sig(self) -> SubrecordSig {
        SubrecordSig(match self {
            Slot::Diffuse => *b"TX00",
            Slot::Normal => *b"TX01",
            Slot::SmoothSpec => *b"TX07",
        })
    }

    /// Vanilla human and ghoul base heads (`basemalehead_*`, `ghoulmalehead_*`).
    fn face_format(self) -> (&'static str, u32) {
        match self {
            Slot::Diffuse => ("BC3_UNORM", 1024),
            Slot::Normal => ("BC5_UNORM", 1024),
            Slot::SmoothSpec => ("BC5_UNORM", 512),
        }
    }
}

fn field<'a>(record: &'a Record, sig: &[u8; 4]) -> Option<&'a FieldValue> {
    record
        .fields
        .iter()
        .find(|entry| &entry.sig.0 == sig)
        .map(|entry| &entry.value)
}

fn form_key_field(record: &Record, sig: &[u8; 4]) -> Option<FormKey> {
    match field(record, sig)? {
        FieldValue::FormKey(fk) if fk.local != 0 => Some(*fk),
        _ => None,
    }
}

fn string_field(record: &Record, sig: &[u8; 4], interner: &StringInterner) -> Option<String> {
    let value = match field(record, sig)? {
        FieldValue::String(sym) => interner.resolve(*sym)?.to_owned(),
        FieldValue::Bytes(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        _ => return None,
    };
    let value = value.trim_matches('\0').trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn flags(record: &Record) -> u64 {
    match field(record, b"DNAM") {
        Some(FieldValue::Uint(value)) => *value,
        Some(FieldValue::Int(value)) => *value as u64,
        _ => 0,
    }
}

/// Texture path relative to `Textures/`, forward slashes, original case.
fn texture_rel(path: &str) -> Option<String> {
    let path = path.trim_matches('\0').trim().replace('\\', "/");
    let path = path.trim_start_matches('/');
    let path = match path.get(..9) {
        Some(prefix) if prefix.eq_ignore_ascii_case("textures/") => &path[9..],
        _ => path,
    };
    (!path.is_empty() && !path.contains(':') && !path.split('/').any(|part| part == ".."))
        .then(|| path.to_owned())
}

/// Diffuse, normal and smooth/spec texture paths of a converted BGSM.
fn material_slots(data_root: &Path, material: &str) -> Result<[Option<String>; 3], String> {
    let material_rel = Path::new("materials").join(material.replace('\\', "/"));
    let path = resolve_path_ci(data_root, &material_rel)
        .ok_or_else(|| format!("converted material {material} is missing"))?;
    let bytes = std::fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let bgsm = materials_native::bgsm::parse(&bytes)
        .map_err(|e| format!("parse {}: {e}", path.display()))?;
    let mut slots = [
        texture_rel(&bgsm.DiffuseTexture),
        texture_rel(&bgsm.NormalTexture),
        texture_rel(&bgsm.SmoothSpecTexture),
    ];
    // Some converted heads ship `_s` beside `_d` while the BGSM slot is empty.
    if slots[2].is_none() {
        slots[2] = slots[0]
            .as_deref()
            .and_then(|diffuse| {
                let stem = diffuse.get(..diffuse.len().checked_sub(6)?)?;
                diffuse[stem.len()..]
                    .eq_ignore_ascii_case("_d.dds")
                    .then(|| format!("{stem}_s.dds"))
            })
            .filter(|spec| texture_file(data_root, spec).is_some());
    }
    Ok(slots)
}

fn texture_file(data_root: &Path, rel: &str) -> Option<PathBuf> {
    resolve_path_ci(data_root, &Path::new("textures").join(rel))
}

/// Head texture sets worn by human or ghoul NPCs, whose face arrays are built
/// from the vanilla base head formats.
fn face_texture_sets(
    session: &mut crate::session::PluginSession,
    ctx: &PhaseCtx<'_>,
) -> Result<HashSet<FormKey>, PhaseError> {
    let interner = &ctx.run.interner;
    let mut sets = HashSet::new();
    for key in session
        .form_keys_of_sig(SigCode(*b"NPC_"), interner)
        .map_err(|e| PhaseError::Internal(e.to_string()))?
    {
        let npc = session
            .record_decoded(&key, &ctx.run.schema_target, interner)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        let (Some(race), Some(texture)) = (form_key_field(&npc, b"RNAM"), form_key_field(&npc, b"FTST"))
        else {
            continue;
        };
        let vanilla = interner
            .resolve(race.plugin)
            .is_some_and(|plugin| plugin.eq_ignore_ascii_case("Fallout4.esm"));
        if vanilla && matches!(race.local, HUMAN_RACE | GHOUL_RACE) {
            sets.insert(texture);
        }
    }
    Ok(sets)
}

/// Re-encode `input` at the base head's format and size. sRGB tags are dropped
/// without converting texels: FO4 shades face textures as stored, like its own
/// UNORM DXT5 heads.
fn write_face_texture(input: &Path, output: &Path, slot: Slot) -> Result<(), String> {
    let mut bytes = std::fs::read(input).map_err(|e| format!("read {}: {e}", input.display()))?;
    untag_srgb(&mut bytes);
    let image = directxtex_native::read_dds_rgba_image_bytes(&bytes)
        .map_err(|e| format!("decode {}: {e}", input.display()))?;
    let (format, size) = slot.face_format();
    let chain = face_mip_chain(image.width, image.height, image.rgba, size)
        .map_err(|e| format!("{}: {e}", input.display()))?;
    let encoded = directxtex_native::encode_dds_from_owned_rgba8_chain(chain, format, true, None)?;
    std::fs::write(output, encoded).map_err(|e| format!("write {}: {e}", output.display()))
}

fn untag_srgb(bytes: &mut [u8]) {
    if bytes.get(84..88) != Some(b"DX10") || bytes.len() < 132 {
        return;
    }
    let format = u32::from_le_bytes(bytes[128..132].try_into().unwrap());
    let unorm = match format {
        29 => 28,
        72 => 71,
        75 => 74,
        78 => 77,
        91 => 87,
        93 => 88,
        99 => 98,
        _ => return,
    };
    bytes[128..132].copy_from_slice(&u32::to_le_bytes(unorm));
}

fn face_mip_chain(
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    size: u32,
) -> Result<Vec<(u32, u32, Vec<u8>)>, String> {
    if width != height || !width.is_power_of_two() {
        return Err(format!("{width}x{height} is not a square power-of-two texture"));
    }
    let (base_size, base) = if width < size {
        (size, upscale_nearest(&rgba, width, size / width))
    } else {
        (width, rgba)
    };
    Ok(directxtex_native::rgba8_box_mip_chain(base_size, base_size, &base)?
        .into_iter()
        .skip_while(|(level_width, _, _)| *level_width > size)
        .collect())
}

fn upscale_nearest(rgba: &[u8], width: u32, factor: u32) -> Vec<u8> {
    let (width, factor) = (width as usize, factor as usize);
    let out_width = width * factor;
    let mut out = Vec::with_capacity(out_width * out_width * 4);
    for y in 0..out_width {
        let row = &rgba[(y / factor) * width * 4..(y / factor + 1) * width * 4];
        for x in 0..out_width {
            out.extend_from_slice(&row[(x / factor) * 4..(x / factor) * 4 + 4]);
        }
    }
    out
}

fn face_rel(rel: &str) -> String {
    format!("{}{FACE_SUFFIX}", rel.strip_suffix(".dds").unwrap_or(rel))
}

/// FO4's human and ghoul base heads: (race folder, file stem). The face array
/// takes their `_d`/`_n`/`_s` as slices, so a replacement must keep FO4's format.
const BASE_HEADS: [(&str, &str); 4] = [
    ("basehumanmale", "basemalehead"),
    ("basehumanfemale", "basefemalehead"),
    ("ghoulmale", "ghoulmalehead"),
    ("ghoulfemale", "ghoulfemalehead"),
];

fn bgsm_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            bgsm_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("bgsm")) {
            out.push(path);
        }
    }
}

/// A real head texture for a FO76 `Actors/Customization/Character/...` path, which
/// FO76 composites at runtime and no archive ships: the same file outside
/// `Customization/`, else the race folder's base head. Copied in from the FO76
/// source when the mod lacks it.
fn customization_replacement(data_root: &Path, source_root: &Path, rel: &str) -> Option<String> {
    let lower = rel.to_ascii_lowercase();
    let start = lower.find("customization/")?;
    let plain = format!("{}{}", &rel[..start], &rel[start + "customization/".len()..]);
    let suffix = &lower[lower.rfind('_')?..];
    let folder = plain.rsplit('/').nth(1)?.to_ascii_lowercase();
    let base = BASE_HEADS
        .iter()
        .find(|(race, _)| *race == folder)
        .map(|(race, stem)| format!("Actors/Character/{race}/{stem}{suffix}"));
    for candidate in std::iter::once(plain).chain(base) {
        if texture_file(data_root, &candidate).is_some() {
            return Some(candidate);
        }
        let source = resolve_path_ci(source_root, &Path::new("textures").join(&candidate))?;
        let output = data_root.join("textures").join(candidate.to_ascii_lowercase());
        std::fs::create_dir_all(output.parent()?).ok()?;
        std::fs::copy(source, &output).ok()?;
        return Some(candidate);
    }
    None
}

/// Points FO76 head BGSMs at real textures. Actors without FaceGen (the player)
/// render the race head material as is, so a `Customization/` slot draws nothing.
fn resolve_customization_heads(
    data_root: &Path,
    source_root: &Path,
    warnings: &mut Vec<String>,
) -> Result<u32, PhaseError> {
    let mut materials = Vec::new();
    bgsm_files(&data_root.join("materials").join("actors"), &mut materials);
    let mut changed = 0;
    for path in materials {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(mut bgsm) = materials_native::bgsm::parse(&bytes) else { continue };
        let mut dirty = false;
        let slots = [Some(&mut bgsm.DiffuseTexture), Some(&mut bgsm.NormalTexture), bgsm.LightingTexture.as_mut()];
        for slot in slots.into_iter().flatten() {
            let Some(rel) = texture_rel(slot) else { continue };
            if !rel.to_ascii_lowercase().contains("/customization/") || texture_file(data_root, &rel).is_some() {
                continue;
            }
            match customization_replacement(data_root, source_root, &rel) {
                Some(real) => {
                    *slot = real;
                    dirty = true;
                }
                None => warnings.push(format!("{}: no real texture for {rel}", path.display())),
            }
        }
        if dirty {
            std::fs::write(&path, materials_native::bgsm::write(&bgsm))
                .map_err(|e| PhaseError::Internal(format!("write {}: {e}", path.display())))?;
            changed += 1;
        }
    }
    Ok(changed)
}

/// Re-encodes FO76 files that replace FO4's base head textures to the face
/// array's format and size; a mismatched slice never loads and shows stale memory.
fn reencode_base_heads(data_root: &Path) -> Result<u32, PhaseError> {
    let mut written = 0;
    for (race, stem) in BASE_HEADS {
        for (slot, suffix) in SLOTS.into_iter().zip(["_d", "_n", "_s"]) {
            let rel = format!("actors/character/{race}/{stem}{suffix}.dds");
            let Some(path) = texture_file(data_root, &rel) else { continue };
            let bytes = std::fs::read(&path).map_err(|e| PhaseError::Internal(format!("read {}: {e}", path.display())))?;
            let probe = directxtex_native::read_dds_probe_bytes(&bytes).map_err(PhaseError::Internal)?;
            let (format, size) = slot.face_format();
            let wanted = match format {
                "BC3_UNORM" => 77,
                _ => 83,
            };
            if probe.dxgi_format == wanted && probe.width == size && probe.height == size {
                continue;
            }
            write_face_texture(&path, &path, slot).map_err(PhaseError::Internal)?;
            written += 1;
        }
    }
    Ok(written)
}

/// The file-only head repairs, also run by the materials-only regen: `(heads, base_heads)`.
pub(crate) fn repair_head_files(
    data_root: &Path,
    source_root: &Path,
    warnings: &mut Vec<String>,
) -> Result<(u32, u32), PhaseError> {
    let heads = resolve_customization_heads(data_root, source_root, warnings)?;
    Ok((heads, reencode_base_heads(data_root)?))
}

pub(super) fn repair(ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
    if ctx.run.source != Game::Fo76 || ctx.run.target != Game::Fo4 {
        return Ok(PhaseReport::default());
    }
    let data_root = ctx.mod_path.join("data");
    let mut head_warnings = Vec::new();
    let (heads, base_heads) = repair_head_files(&data_root, ctx.source_extracted_dir, &mut head_warnings)?;
    let mut session = open_session(ctx.run.target_handle_id, None)
        .map_err(|e| PhaseError::Internal(e.to_string()))?;
    let output_plugin = ctx
        .run
        .interner
        .intern(&session.target_slot().parsed.plugin_name);
    let face_sets = face_texture_sets(&mut session, ctx)?;

    let mut report = PhaseReport::default();
    let mut warnings = head_warnings;
    let mut written = HashSet::new();
    for key in session
        .form_keys_of_sig(SigCode(*b"TXST"), &ctx.run.interner)
        .map_err(|e| PhaseError::Internal(e.to_string()))?
    {
        ctx.check_cancel()?;
        if key.plugin != output_plugin {
            continue;
        }
        let mut texture = session
            .record_decoded(&key, &ctx.run.schema_target, &ctx.run.interner)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        if flags(&texture) & FACEGEN_TEXTURES == 0
            || SLOTS.iter().any(|slot| field(&texture, &slot.sig().0).is_some())
        {
            continue;
        }
        let Some(material) = string_field(&texture, b"MNAM", &ctx.run.interner) else {
            continue;
        };
        let slots = match material_slots(&data_root, &material) {
            Ok(slots) => slots,
            Err(error) => {
                warnings.push(format!("TXST {:06X}: {error}", key.local));
                continue;
            }
        };
        let face = face_sets.contains(&key);
        let mut entries = Vec::new();
        for (slot, rel) in SLOTS.into_iter().zip(slots) {
            let Some(mut rel) = rel else { continue };
            if face {
                let Some(input) = texture_file(&data_root, &rel) else {
                    warnings.push(format!("TXST {:06X}: face texture {rel} is missing", key.local));
                    continue;
                };
                let output_rel = face_rel(&rel);
                let output = input.with_file_name(
                    Path::new(&output_rel).file_name().expect("texture file name"),
                );
                if written.insert(output.clone()) {
                    write_face_texture(&input, &output, slot).map_err(PhaseError::Internal)?;
                }
                rel = output_rel;
            }
            entries.push(FieldEntry {
                sig: slot.sig(),
                value: FieldValue::String(ctx.run.interner.intern(&rel.replace('/', "\\"))),
            });
        }
        if entries.is_empty() {
            continue;
        }
        let insert_at = texture
            .fields
            .iter()
            .rposition(|entry| matches!(&entry.sig.0, b"OBND" | b"EDID"))
            .map_or(0, |index| index + 1);
        texture.fields.insert_many(insert_at, entries);
        if session
            .replace_record_contents(texture, &ctx.run.schema_target, &ctx.run.interner)
            .map_err(|e| PhaseError::Internal(e.to_string()))?
        {
            report.records_changed += 1;
        }
    }
    report.assets_written = written.len() as u32 + heads + base_heads;
    report.warnings = warnings.len() as u32;
    for message in warnings {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "finalize_plugin_records",
            level: LogLevel::Warn,
            message,
        });
    }
    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
        phase: "finalize_plugin_records",
        level: LogLevel::Info,
        message: format!(
            "finalize_plugin_records: {} face texture set(s) filled from their materials, {} face texture(s) re-encoded, \
             {heads} head material(s) pointed at real textures, {base_heads} FO4 base head texture(s) re-encoded",
            report.records_changed,
            written.len()
        ),
    });
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase::Phase;
    use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
    use crate::schema::AuthoringSchema;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_no_py,
    };
    use std::sync::{Arc, atomic::AtomicBool};

    const OUTPUT: &str = "SeventySix.esm";

    fn run_final(handle: u64, root: &Path) -> PhaseReport {
        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: 0,
            target_handle_id: handle,
            master_handle_ids: vec![],
            config: RunConfig::default(),
        })
        .unwrap();
        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let params = serde_json::json!({"regenerate_modt": false, "rebuild_cell_offsets": false});
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
        })
        .unwrap();
        drop_run(id).unwrap();
        report
    }

    fn write_dds(path: &Path, format: &str, pixel: [u8; 4]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let bytes = directxtex_native::encode_dds_from_owned_rgba8_chain(
            vec![(4, 4, pixel.repeat(16))],
            format,
            false,
            None,
        )
        .unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn write_bgsm(path: &Path, diffuse: &str, normal: &str, spec: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut material = materials_native::bgsm::BgsmData::default();
        material.header.signature = materials_native::bgsm::BGSM_SIGNATURE;
        material.header.version = 2;
        material.DiffuseTexture = diffuse.into();
        material.NormalTexture = normal.into();
        material.SmoothSpecTexture = spec.into();
        std::fs::write(path, materials_native::bgsm::write(&material)).unwrap();
    }

    fn texture_txst(key: FormKey, material: &str, interner: &StringInterner) -> Record {
        let mut record = Record::new(SigCode(*b"TXST"), key);
        record.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"DNAM"),
                value: FieldValue::Uint(FACEGEN_TEXTURES),
            },
            FieldEntry {
                sig: SubrecordSig(*b"MNAM"),
                value: FieldValue::String(interner.intern(material)),
            },
        ]);
        record
    }

    #[test]
    fn face_texture_sets_take_material_slots_and_base_head_formats() {
        let directory = tempfile::tempdir().unwrap();
        let data = directory.path().join("data");
        let textures = data.join("textures/actors/corpse/male");
        write_dds(&textures.join("head_d.dds"), "R8G8B8A8_UNORM_SRGB", [200, 100, 50, 255]);
        write_dds(&textures.join("head_n.dds"), "R8G8B8A8_UNORM", [128, 128, 255, 255]);
        write_dds(&textures.join("head_s.dds"), "R8G8B8A8_UNORM", [90, 30, 0, 255]);
        write_bgsm(
            &data.join("materials/actors/corpse/malehead.bgsm"),
            "Actors/Corpse/Male/Head_d.dds",
            "Actors/Corpse/Male/Head_n.dds",
            "",
        );
        write_bgsm(
            &data.join("materials/actors/corpse/malebody.bgsm"),
            "Actors/Corpse/Male/Body_d.dds",
            "Actors/Corpse/Male/Body_n.dds",
            "Actors/Corpse/Male/Body_s.dds",
        );

        let interner = StringInterner::new();
        let key = |local| FormKey {
            plugin: interner.intern(OUTPUT),
            local,
        };
        let mut npc = Record::new(SigCode(*b"NPC_"), key(0xA00));
        npc.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"RNAM"),
                value: FieldValue::FormKey(FormKey {
                    plugin: interner.intern("Fallout4.esm"),
                    local: HUMAN_RACE,
                }),
            },
            FieldEntry {
                sig: SubrecordSig(*b"FTST"),
                value: FieldValue::FormKey(key(0x900)),
            },
        ]);
        let records = vec![
            texture_txst(key(0x900), r"actors\corpse\malehead.bgsm", &interner),
            texture_txst(key(0x901), r"actors\corpse\malebody.bgsm", &interner),
            npc,
        ];
        let handle = plugin_handle_new_no_py(OUTPUT, Some("fo4"));
        plugin_handle_add_master_native(handle, "Fallout4.esm", None).unwrap();
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        open_session(handle, None)
            .unwrap()
            .add_records(records, &schema, &interner)
            .unwrap();

        let report = run_final(handle, directory.path());
        assert_eq!((report.records_changed, report.assets_written), (2, 3));

        let mut session = open_session(handle, None).unwrap();
        let mut slots = |local| {
            let record = session.record_decoded(&key(local), &schema, &interner).unwrap();
            SLOTS.map(|slot| string_field(&record, &slot.sig().0, &interner))
        };
        assert_eq!(
            slots(0x900),
            [
                Some(r"Actors\Corpse\Male\Head_d_bacup_face.dds".into()),
                Some(r"Actors\Corpse\Male\Head_n_bacup_face.dds".into()),
                Some(r"Actors\Corpse\Male\Head_s_bacup_face.dds".into()),
            ]
        );
        assert_eq!(
            slots(0x901),
            [
                Some(r"Actors\Corpse\Male\Body_d.dds".into()),
                Some(r"Actors\Corpse\Male\Body_n.dds".into()),
                Some(r"Actors\Corpse\Male\Body_s.dds".into()),
            ]
        );
        drop(session);

        for (name, size, format, mips) in [
            ("head_d_bacup_face.dds", 1024, 77, 11),
            ("head_n_bacup_face.dds", 1024, 83, 11),
            ("head_s_bacup_face.dds", 512, 83, 10),
        ] {
            let probe = directxtex_native::read_dds_probe(&textures.join(name)).unwrap();
            assert_eq!(
                (probe.width, probe.height, probe.dxgi_format, probe.mip_levels),
                (size, size, format, mips),
                "{name}"
            );
        }
        let diffuse =
            directxtex_native::read_dds_rgba_image(&textures.join("head_d_bacup_face.dds")).unwrap();
        for (channel, expected) in diffuse.rgba[..3].iter().zip([200_i32, 100, 50]) {
            assert!((i32::from(*channel) - expected).abs() <= 8, "{:?}", &diffuse.rgba[..4]);
        }

        let repeated = run_final(handle, directory.path());
        assert_eq!((repeated.records_changed, repeated.assets_written), (0, 0));
        assert!(plugin_handle_close_native(handle));
    }

    #[test]
    fn customization_heads_get_real_textures_and_base_heads_keep_fo4_formats() {
        let directory = tempfile::tempdir().unwrap();
        let (data, source) = (directory.path().join("data"), directory.path().join("fo76"));
        let male = data.join("textures/actors/character/basehumanmale");
        write_dds(&male.join("basemalehead_d.dds"), "BC1_UNORM_SRGB", [200, 150, 120, 255]);
        write_dds(&source.join("textures/actors/character/basehumanmale/basemalehead_l.dds"), "BC1_UNORM", [90, 255, 0, 255]);
        let material = data.join("materials/actors/character/basehumanmale/basehumanskinhead.bgsm");
        write_bgsm(&material, "Actors/Customization/Character/BaseHumanMale/BaseMaleHead_d.dds", "", "");
        let mut bgsm = materials_native::bgsm::parse(&std::fs::read(&material).unwrap()).unwrap();
        bgsm.NormalTexture = "Actors/Customization/Character/BaseHumanMale/Mayor_n.dds".into();
        bgsm.header.version = 22;
        bgsm.LightingTexture = Some("Actors/Customization/Character/BaseHumanMale/BaseMaleHead_l.dds".into());
        std::fs::write(&material, materials_native::bgsm::write(&bgsm)).unwrap();

        let mut warnings = Vec::new();
        assert_eq!(resolve_customization_heads(&data, &source, &mut warnings).unwrap(), 1);
        let bgsm = materials_native::bgsm::parse(&std::fs::read(&material).unwrap()).unwrap();
        assert_eq!(bgsm.DiffuseTexture.trim_end_matches('\0'), "Actors/Character/BaseHumanMale/BaseMaleHead_d.dds");
        // The lighting map was only in the FO76 source; it is copied in.
        assert_eq!(bgsm.LightingTexture.as_deref().unwrap().trim_end_matches('\0'),"Actors/Character/BaseHumanMale/BaseMaleHead_l.dds");
        assert!(male.join("basemalehead_l.dds").is_file());
        // Mayor_n exists nowhere and falls back to nothing (no base `_n` either), with a warning.
        assert_eq!(warnings.len(), 1, "{warnings:?}");

        assert_eq!(reencode_base_heads(&data).unwrap(), 1);
        let probe = directxtex_native::read_dds_probe_bytes(&std::fs::read(male.join("basemalehead_d.dds")).unwrap()).unwrap();
        assert_eq!((probe.dxgi_format, probe.width), (77, 1024), "BC3_UNORM at the base head's 1024");
        assert_eq!(reencode_base_heads(&data).unwrap(), 0, "already in FO4's format");
    }
}
