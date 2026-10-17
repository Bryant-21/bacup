use std::path::Path;
use std::time::Instant;

use serde_json::{Value, json};

use super::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};

const MAP: &str = "textures/interface/pip-boy/papermap_city_d.dds";
const MILITARY: &str = "textures/interface/pip-boy/military_map_d.dds";
const PACK: &str = "F4SE/Plugins/B21_FullScreenMap/maps/appalachia";

pub struct PipboyMapPhase;

fn encode(pixels: &[u8], width: u32, height: u32, path: &Path, format: &str, gpu: bool, timings: &mut Vec<Value>) -> Result<(), String> {
    let started = Instant::now();
    let (result, profile) = directxtex_native::profiling::capture(|| {
        directxtex_native::write_dds_rgba_image_gpu(path, width, height, pixels, format, true, gpu)
    });
    result?;
    timings.push(json!({"operation": "encode", "path": path, "width": width, "height": height,
        "seconds": started.elapsed().as_secs_f64(), "mips_seconds": profile.mips_ns as f64 / 1e9,
        "encode_seconds": profile.encode_ns as f64 / 1e9, "gpu_seconds": profile.gpu_wait_ns as f64 / 1e9,
        "write_seconds": profile.write_ns as f64 / 1e9, "gpu_attempts": profile.gpu_attempts, "gpu_failures": profile.gpu_failures}));
    Ok(())
}

fn decode(path: &Path, timings: &mut Vec<Value>) -> Result<(u32, u32, Vec<u8>), String> {
    let started = Instant::now();
    let image = directxtex_native::read_dds_rgba_image(path)?;
    timings.push(json!({"operation": "decode", "path": path, "seconds": started.elapsed().as_secs_f64()}));
    Ok((image.width, image.height, image.rgba))
}

fn write_map(pixels: &[u8], width: u32, height: u32, path: &Path, timings: &mut Vec<Value>) -> Result<(), String> {
    let aligned_width = width.div_ceil(4) * 4;
    let aligned_height = height.div_ceil(4) * 4;
    if (width, height) == (aligned_width, aligned_height) {
        return encode(pixels, width, height, path, "BC7_UNORM", true, timings);
    }
    let mut padded = vec![0u8; aligned_width as usize * aligned_height as usize * 4];
    for (source, target) in pixels.chunks_exact(width as usize * 4).zip(padded.chunks_exact_mut(aligned_width as usize * 4)) {
        target[..source.len()].copy_from_slice(source);
    }
    encode(&padded, aligned_width, aligned_height, path, "BC7_UNORM", true, timings)
}

pub fn generate(source: &Path, output: &Path, log: &dyn Fn(LogLevel, String)) -> Result<u32, String> {
    let map_source = source.join(MAP);
    if !map_source.is_file() {
        log(LogLevel::Warn, format!("FO76 Appalachia Pip-Boy map texture not found: {}", map_source.display()));
        return Ok(0);
    }
    let started = Instant::now();
    let mut timings = Vec::new();
    let pack = output.join(PACK);
    let (width, height, pixels) = decode(&map_source, &mut timings)?;
    write_map(&pixels, width, height, &pack.join("map.dds"), &mut timings)?;
    let resize_started = Instant::now();
    let pixels = crate::image_resize::lanczos_rgba(pixels, width as usize, height as usize, 2048, 2048);
    timings.push(json!({"operation": "resize", "seconds": resize_started.elapsed().as_secs_f64()}));
    let mut manifest = json!({"worldspace": "Appalachia", "title": "APPALACHIA", "image": "map.dds", "discovery": "proximity",
        "calibration": {"mode": "survey", "centerX": 340, "centerY": 135, "maxRange": 582500, "mapPx": 4096}});
    let mut written = 3;
    let military = source.join(MILITARY);
    if military.is_file() {
        let (width, height, pixels) = decode(&military, &mut timings)?;
        write_map(&pixels, width, height, &pack.join("military.dds"), &mut timings)?;
        manifest["nukeImage"] = json!("military.dds");
        written += 1;
    } else {
        log(LogLevel::Warn, format!("FO76 military map not found for nuke targeting: {}", military.display()));
    }
    std::fs::write(pack.join("map.json"), serde_json::to_string_pretty(&manifest).unwrap() + "\n").map_err(|e| e.to_string())?;
    encode(&pixels, 2048, 2048, &output.join("data").join(MAP), "DXT1", false, &mut timings)?;
    let report = json!({"seconds": started.elapsed().as_secs_f64(), "operations": timings});
    let reports = output.join("conversion");
    std::fs::create_dir_all(&reports).map_err(|e| e.to_string())?;
    std::fs::write(reports.join("pipboy_map_timing.json"), serde_json::to_vec_pretty(&report).unwrap()).map_err(|e| e.to_string())?;
    log(LogLevel::Info, format!("pipboy_map: {report}"));
    log(LogLevel::Info, format!("Wrote FO4-compatible Appalachia Pip-Boy map texture: {}", output.join("data").join(MAP).display()));
    log(LogLevel::Info, format!("Wrote generated Appalachia fullscreen map pack: {}", pack.display()));
    Ok(written)
}

impl Phase for PipboyMapPhase {
    fn name(&self) -> &'static str { "finalize_fo76_maps" }
    fn requires_source_plugin(&self) -> bool { false }
    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        ctx.check_cancel()?;
        let written = generate(ctx.source_extracted_dir, ctx.mod_path, &|level, message| {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log { phase: "finalize_fo76_maps", level, message });
        }).map_err(PhaseError::Internal)?;
        Ok(PhaseReport { assets_written: written, ..PhaseReport::default() })
    }
}
