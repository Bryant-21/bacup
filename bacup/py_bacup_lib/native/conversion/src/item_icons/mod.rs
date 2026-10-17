pub mod assets;
pub mod geometry;
pub mod records;
pub mod render;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

use base64::Engine;
use nif_core_native::model::NifFile;
use rayon::prelude::*;
use serde::Deserialize;
use serde_json::{Value, json};

use assets::{Assets, GameData};
use records::{Item, icon_model};

#[derive(Deserialize)]
pub struct Request {
    pub plugins: Vec<(String, Option<String>)>,
    pub data: GameData,
    pub mod_root: PathBuf,
    pub workers: Option<usize>,
    pub report: Option<PathBuf>,
}

struct Rendered {
    detail: Value,
    alpha: Option<Vec<u8>>,
}

pub fn encoded(alpha: &[u8]) -> Result<String, String> {
    let mut size = unsafe { libz_sys::compressBound(alpha.len() as libz_sys::uLong) };
    let mut output = vec![0; size as usize];
    let result = unsafe {
        libz_sys::compress2(
            output.as_mut_ptr(),
            &mut size,
            alpha.as_ptr(),
            alpha.len() as libz_sys::uLong,
            9,
        )
    };
    if result != libz_sys::Z_OK {
        return Err(format!("icon compression failed: {result}"));
    }
    output.truncate(size as usize);
    Ok(base64::engine::general_purpose::STANDARD.encode(output))
}

fn render_model(model: &str, kind: &str, assets: &Assets) -> Result<Rendered, String> {
    let names: Option<Vec<_>> = model.split('|').map(|part| assets.model(part)).collect();
    let Some(names) = names else {
        return Ok(Rendered {
            detail: json!({"error":"missing model"}),
            alpha: None,
        });
    };
    let path = names.join("|");
    let mut triangles = Vec::new();
    let mut skipped = Vec::new();
    let prepared = (|| -> Result<(), String> {
        for name in names {
            let bytes = assets
                .read(&name)?
                .ok_or_else(|| format!("missing model: {name}"))?;
            let nif = NifFile::from_bytes(&bytes, None).map_err(|e| e.to_string())?;
            let (points, reasons) = geometry::visible_triangles(&nif, assets)?;
            triangles.extend(points);
            skipped.extend(reasons);
        }
        Ok(())
    })();
    if let Err(error) = prepared {
        return Ok(Rendered {
            detail: json!({"path":path,"error":format!("unreadable model: {error}")}),
            alpha: None,
        });
    }
    let mut detail = json!({"path":path,"skipped":skipped,"triangles":triangles.len()});
    if triangles.is_empty() {
        detail["error"] = "no drawable geometry".into();
        return Ok(Rendered {
            detail,
            alpha: None,
        });
    }
    let (view, alpha) = render::item_icon(&triangles, kind)?;
    if alpha.iter().any(|&v| v != 0) {
        detail["view"] = view.into();
        Ok(Rendered {
            detail,
            alpha: Some(alpha),
        })
    } else {
        detail["error"] = "empty render".into();
        Ok(Rendered {
            detail,
            alpha: None,
        })
    }
}

fn kind(item: &Item) -> &str {
    if matches!(item.signature.as_str(), "WEAP" | "ARMO" | "ALCH") {
        &item.signature
    } else {
        "item"
    }
}

pub fn generate(request: Request) -> Result<Value, String> {
    let started = Instant::now();
    let assets = Assets::new(&request.data)?;
    let inventory_seconds = started.elapsed().as_secs_f64();
    let collect_started = Instant::now();
    let items = records::collect(&request.plugins)?;
    let collect_seconds = collect_started.elapsed().as_secs_f64();
    let jobs: Vec<_> = items
        .iter()
        .filter_map(|item| icon_model(item).map(|model| (model, kind(item).to_string())))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let render_started = Instant::now();
    let results: Vec<_> = crate::worker_pool::install(request.workers, || {
        jobs.par_iter()
            .map(|(model, kind)| render_model(model, kind, &assets))
            .collect::<Result<Vec<_>, String>>()
    })
    .map_err(|e| e.to_string())??;
    let render_seconds = render_started.elapsed().as_secs_f64();
    let catalog_started = Instant::now();
    let mut icons = Vec::new();
    let mut shared = HashMap::new();
    let mut index = BTreeMap::new();
    for (job, result) in jobs.iter().zip(&results) {
        if let Some(alpha) = &result.alpha {
            let number = if let Some(&number) = shared.get(alpha) {
                number
            } else {
                let number = icons.len();
                icons.push(json!({"view":result.detail["view"],"alpha":encoded(alpha)?}));
                shared.insert(alpha.clone(), number);
                number
            };
            index.insert(job.clone(), number);
        }
    }
    let mut sorted: Vec<_> = items.iter().collect();
    sorted.sort_by(|a, b| a.form.cmp(&b.form));
    let mut forms: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut fallback_forms = BTreeMap::new();
    for item in sorted {
        let Some(model) = icon_model(item) else {
            continue;
        };
        let Some(&number) = index.get(&(model, kind(item).into())) else {
            continue;
        };
        let Some((object, plugin)) = item.form.split_once(':') else {
            return Err(format!("Invalid icon form: {}", item.form));
        };
        let section = if item.signature == "WEAP" && !item.thrown {
            &mut fallback_forms
        } else {
            &mut forms
        };
        section
            .entry(plugin.to_string())
            .or_insert_with(BTreeMap::new)
            .insert(object.to_string(), number);
    }
    let output = request
        .mod_root
        .join("F4SE/Plugins/B21_TalesFromAppalachia/item_icons.json");
    let catalog = json!({"version":1,"width":128,"height":128,"encoding":"zlib-alpha8","icons":icons,"forms":forms,"fallback_forms":fallback_forms});
    write_json(&output, &catalog, false)?;
    if let Some(path) = request.report {
        let reports: BTreeMap<_, _> = jobs.iter().zip(&results).collect();
        let flags: BTreeMap<_, _> = jobs
            .iter()
            .zip(&results)
            .filter_map(|(job, result)| {
                result
                    .alpha
                    .as_deref()
                    .map(|alpha| (job, icon_flags(alpha)))
            })
            .collect();
        let mut rows = Vec::new();
        let mut report_items: Vec<_> = items.iter().collect();
        report_items.sort_by(|a, b| a.form.cmp(&b.form));
        for item in report_items {
            let model = icon_model(item);
            let job = (model.clone().unwrap_or_default(), kind(item).into());
            let result = reports.get(&job).copied();
            let detail = result
                .map(|r| r.detail.clone())
                .unwrap_or_else(|| json!({"error":"category glyph"}));
            let mut row = serde_json::to_value(item).map_err(|e| e.to_string())?;
            row["icon"] = index.get(&job).map(|v| json!(v)).unwrap_or(Value::Null);
            for key in ["view", "error", "path", "triangles"] {
                row[key] = detail[key].clone();
            }
            row["skipped"] = detail.get("skipped").cloned().unwrap_or_else(|| json!([]));
            row["flags"] = json!(flags.get(&job).cloned().unwrap_or_default());
            rows.push(row);
        }
        write_json(&path, &json!(rows), true)?;
    }
    Ok(
        json!({"output":output,"icons":icons.len(),"primary_forms":forms.values().map(BTreeMap::len).sum::<usize>(),
        "fallback_forms":fallback_forms.values().map(BTreeMap::len).sum::<usize>(),"eligible_forms":items.len(),
        "timings":{"inventory":inventory_seconds,"records":collect_seconds,"render":render_seconds,
        "catalog":catalog_started.elapsed().as_secs_f64(),"total":started.elapsed().as_secs_f64()}}),
    )
}

fn write_json(path: &Path, value: &Value, pretty: bool) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let bytes = if pretty {
        serde_json::to_vec_pretty(value)
    } else {
        serde_json::to_vec(value)
    }
    .map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

pub fn icon_flags(alpha: &[u8]) -> Vec<&'static str> {
    let covered: Vec<_> = alpha
        .iter()
        .enumerate()
        .filter(|(_, v)| **v > 0)
        .map(|(i, _)| i)
        .collect();
    if covered.is_empty() {
        return vec!["empty"];
    }
    let bounds = |pixels: &[usize]| {
        let minx = pixels.iter().map(|i| i % 128).min().unwrap();
        let maxx = pixels.iter().map(|i| i % 128).max().unwrap();
        let miny = pixels.iter().map(|i| i / 128).min().unwrap();
        let maxy = pixels.iter().map(|i| i / 128).max().unwrap();
        (maxx - minx + 1, maxy - miny + 1)
    };
    let (width, height) = bounds(&covered);
    let count = covered.len() as f64;
    let meanx = covered.iter().map(|i| (i % 128) as f64).sum::<f64>() / count;
    let meany = covered.iter().map(|i| (i / 128) as f64).sum::<f64>() / count;
    let mut xx = 0.;
    let mut yy = 0.;
    let mut xy = 0.;
    if covered.len() > 2 {
        for i in &covered {
            let x = (*i % 128) as f64 - meanx;
            let y = (*i / 128) as f64 - meany;
            xx += x * x;
            yy += y * y;
            xy += x * y;
        }
    }
    let disc = ((xx - yy).powi(2) + 4. * xy * xy).sqrt();
    let mut flags = Vec::new();
    if (width.min(height) as f64) < 0.15 * width.max(height) as f64
        || xx + yy - disc < 0.15f64.powi(2) * (xx + yy + disc)
    {
        flags.push("thin");
    }
    if count < 0.03 * 128. * 128. {
        flags.push("tiny");
    }
    if count < 0.08 * (width * height) as f64 {
        flags.push("sparse");
    }
    let mut seen = vec![false; alpha.len()];
    let mut largest = Vec::new();
    for &start in &covered {
        if seen[start] {
            continue;
        }
        let mut pixels = vec![start];
        seen[start] = true;
        let mut at = 0;
        while at < pixels.len() {
            let i = pixels[at];
            at += 1;
            for neighbor in [
                i.checked_sub(128),
                (i + 128 < alpha.len()).then_some(i + 128),
                (i % 128 > 0).then(|| i - 1),
                (i % 128 < 127).then_some(i + 1),
            ]
            .into_iter()
            .flatten()
            {
                if !seen[neighbor] && alpha[neighbor] > 0 {
                    seen[neighbor] = true;
                    pixels.push(neighbor);
                }
            }
        }
        if pixels.len() > largest.len() {
            largest = pixels;
        }
    }
    if (largest.len() as f64) < 0.9 * count {
        flags.push("fragmented");
    }
    let (main_width, main_height) = bounds(&largest);
    if (main_width.max(main_height) as f64) < 0.5 * width.max(height) as f64 {
        flags.push("scattered");
    }
    flags
}
