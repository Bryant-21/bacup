use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use crate::script_patches::{self, Patch};
use crate::script_references::{
    self, ScriptRecord, ScriptReferenceInspection, ScriptResolutionEvidence, SubrecordPayload,
};
use bytes::Bytes;
use esp_authoring_core::plugin_runtime::{self, ParsedItem, ParsedSubrecord};
use indexmap::IndexMap;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use smol_str::SmolStr;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;
use std::time::Instant;

const PHASE: &str = "convert_scripts";
const SKIP: [&str; 4] = [
    "player",
    "creatures:mirelurkqueenracescript",
    "creatures:radscorpionracescript",
    "creatures:smbehemothracescript",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Resolution {
    pub script_name: String,
    pub status: String,
    pub pex_path: Option<PathBuf>,
    pub message: String,
}

impl Resolution {
    fn new(name: &str, status: &str, path: Option<PathBuf>, message: impl Into<String>) -> Self {
        Self {
            script_name: name.to_owned(),
            status: status.to_owned(),
            pex_path: path,
            message: message.into(),
        }
    }
    fn ok(&self) -> bool {
        // "unsupported" is the deliberate SKIP outcome (VMAD stripped on purpose), not a failure.
        matches!(self.status.as_str(), "compiled" | "target" | "unsupported")
    }
}

#[derive(Clone)]
struct PatchEntry {
    name: String,
    source: String,
    patch: Patch,
}

pub fn relative_path(name: &str, extension: &str) -> PathBuf {
    let normalized = name.replace(['\\', '/'], ":");
    let mut path: PathBuf = normalized
        .split(':')
        .filter(|part| !part.is_empty())
        .collect();
    if path.as_os_str().is_empty() {
        path.push(name);
    }
    path.set_extension(extension.trim_start_matches('.'));
    path
}

fn walk_files(root: &Path, suffix: &str, output: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut directories = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() && !kind.is_symlink() {
            directories.push(path);
        } else if path
            .extension()
            .is_some_and(|x| x.to_string_lossy().eq_ignore_ascii_case(suffix))
        {
            output.push(path);
        }
    }
    for directory in directories {
        walk_files(&directory, suffix, output);
    }
}

fn script_name(path: &Path, root: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path).with_extension("");
    let mut parts = relative
        .iter()
        .map(|x| x.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    if parts
        .first()
        .is_some_and(|x| x.eq_ignore_ascii_case("client") || x.eq_ignore_ascii_case("server"))
    {
        parts.remove(0);
    }
    parts.join(":")
}

fn dedupe_paths(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|p| seen.insert(p.to_string_lossy().replace('\\', "/").to_lowercase()))
        .collect()
}

fn source_roots(root: Option<&Path>, extras: &[PathBuf]) -> Vec<PathBuf> {
    dedupe_paths(
        root.into_iter()
            .chain(extras.iter().map(PathBuf::as_path))
            .flat_map(|root| {
                [
                    root.join("scripts/client"),
                    root.join("Scripts/Client"),
                    root.join("Scripts"),
                    root.join("Data/Scripts"),
                ]
            }),
    )
}

fn pex_index(roots: &[PathBuf]) -> IndexMap<String, (String, PathBuf)> {
    let mut result = IndexMap::new();
    for root in roots {
        let mut paths = Vec::new();
        walk_files(root, "pex", &mut paths);
        for path in paths {
            let name = script_name(&path, root);
            result
                .entry(script_patches::key(&name))
                .or_insert((name, path));
        }
    }
    result
}

fn target_indexes(
    ctx: &PhaseCtx<'_>,
) -> (
    IndexMap<String, Option<PathBuf>>,
    IndexMap<String, Option<PathBuf>>,
) {
    let mut target = IndexMap::new();
    if let Some(root) = ctx.target_data_dir {
        target.extend(
            pex_index(&[root.join("Scripts")])
                .into_iter()
                .map(|(k, (_, p))| (k, Some(p))),
        );
    }
    let mut authoritative = IndexMap::new();
    if let Some(root) = ctx.target_extracted_dir {
        authoritative.extend(
            pex_index(&dedupe_paths([root.join("Scripts"), root.join("scripts")]))
                .into_iter()
                .map(|(k, (_, p))| (k, Some(p))),
        );
    }
    for (key, path) in &authoritative {
        target.entry(key.clone()).or_insert_with(|| path.clone());
    }
    if let Some(store) = ctx.run.target_assets.as_ref() {
        for path in store.list_assets("scripts/", ".pex") {
            let name = script_patches::key(&script_name(Path::new(&path), Path::new("scripts")));
            target.entry(name.clone()).or_insert(None);
            authoritative.entry(name).or_insert(None);
        }
    }
    (target, authoritative)
}

fn patches(root: &Path) -> Result<IndexMap<String, PatchEntry>, String> {
    let mut paths = Vec::new();
    walk_files(root, "psc", &mut paths);
    let mut result = IndexMap::new();
    let mut seen_paths = HashMap::new();
    for path in paths {
        let name = path
            .strip_prefix(root)
            .unwrap()
            .with_extension("")
            .iter()
            .map(|x| x.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(":");
        let key = script_patches::key(&name);
        if let Some(previous) = seen_paths.insert(key.clone(), path.clone()) {
            return Err(format!(
                "duplicate case-insensitive Papyrus patch path: {} and {}",
                previous.display(),
                path.display()
            ));
        }
        let source = read_text(&path)?;
        let patch = Patch::parse(&source)?;
        result.insert(
            key,
            PatchEntry {
                name,
                source,
                patch,
            },
        );
    }
    Ok(result)
}

fn log(ctx: &PhaseCtx<'_>, level: LogLevel, message: impl Into<String>) {
    let _ = ctx.run.event_tx.send(PhaseEvent::Log {
        phase: PHASE,
        level,
        message: message.into(),
    });
}

fn record_candidates(items: &[ParsedItem], output: &mut Vec<u32>) {
    for item in items {
        match item {
            ParsedItem::Group(group) => record_candidates(&group.children, output),
            ParsedItem::Record(record) => {
                if !record
                    .subrecords
                    .iter()
                    .any(|s| matches!(s.signature.as_str(), "VMAD" | "CTDA"))
                {
                    continue;
                }
                output.push(record.form_id);
            }
        }
    }
}

fn read_record(handle: u64, form_id: u32) -> Result<Option<ScriptRecord>, String> {
    let mut store = plugin_runtime::plugin_handle_store_ref()
        .lock()
        .map_err(|e| e.to_string())?;
    let slot = store
        .get_mut(&handle)
        .ok_or_else(|| format!("unknown plugin handle: {handle}"))?;
    let index = plugin_runtime::ensure_records_section(slot);
    Ok(index
        .record(&slot.parsed, form_id)
        .map(|record| ScriptRecord {
            form_id: record.form_id,
            signature: record.signature.to_string(),
            editor_id: record
                .subrecords
                .iter()
                .find(|s| s.signature == "EDID")
                .map(|s| {
                    let end = s.data.iter().position(|b| *b == 0).unwrap_or(s.data.len());
                    encoding_rs::WINDOWS_1252
                        .decode(&s.data[..end])
                        .0
                        .into_owned()
                })
                .unwrap_or_default(),
            subrecords: record
                .subrecords
                .iter()
                .map(|s| {
                    (
                        s.signature.to_string(),
                        s.data.to_vec(),
                        s.semantic_type.clone(),
                    )
                })
                .collect(),
        }))
}

fn set_subrecords(
    handle: u64,
    form_id: u32,
    subrecords: Vec<SubrecordPayload>,
) -> Result<bool, String> {
    let mut store = plugin_runtime::plugin_handle_store_ref()
        .lock()
        .map_err(|e| e.to_string())?;
    let slot = store
        .get_mut(&handle)
        .ok_or_else(|| format!("unknown plugin handle: {handle}"))?;
    fn locate(items: &mut [ParsedItem], form_id: u32) -> Option<&mut plugin_runtime::ParsedRecord> {
        for item in items {
            match item {
                ParsedItem::Record(record) if record.form_id == form_id => return Some(record),
                ParsedItem::Group(group) => {
                    if let Some(record) = locate(&mut group.children, form_id) {
                        return Some(record);
                    }
                }
                _ => {}
            }
        }
        None
    }
    let record = locate(&mut slot.parsed.root_items, form_id)
        .ok_or_else(|| format!("unknown record form_id: {form_id:08X}"))?;
    record.subrecords = subrecords
        .into_iter()
        .map(|(signature, data, semantic_type)| ParsedSubrecord {
            signature: SmolStr::new(signature),
            data: Bytes::from(data),
            semantic_type,
        })
        .collect();
    slot.apply_write_effect(&plugin_runtime::WriteEffect::RecordContents {
        form_ids: smallvec::smallvec![form_id],
    });
    Ok(true)
}

fn inspect(ctx: &mut PhaseCtx<'_>) -> Result<ScriptReferenceInspection, String> {
    ctx.run.script_reference_state = None;
    let handle = ctx.run.target_handle_id;
    let mut candidates = Vec::new();
    {
        let store = plugin_runtime::plugin_handle_store_ref()
            .lock()
            .map_err(|e| e.to_string())?;
        let slot = store
            .get(&handle)
            .ok_or_else(|| format!("unknown plugin handle: {handle}"))?;
        record_candidates(&slot.parsed.root_items, &mut candidates);
    }
    script_references::inspect_script_references(
        candidates,
        &ctx.run.config.output_plugin_name,
        true,
        |id| read_record(handle, id),
        |id, subrecords| set_subrecords(handle, id, subrecords),
    )
}

fn remove_outputs(root: &Path, name: &str) -> Vec<String> {
    let mut errors = Vec::new();
    for (sub, extension) in [("Scripts/Source/User", "psc"), ("data/Scripts", "pex")] {
        let path = root.join(sub).join(relative_path(name, extension));
        if path.is_file() {
            if let Err(error) = fs::remove_file(&path) {
                errors.push(format!("{}: {error}", path.display()));
            }
        }
    }
    errors
}

fn clean(ctx: &PhaseCtx<'_>, name: &str) {
    for error in remove_outputs(ctx.mod_path, name) {
        log(
            ctx,
            LogLevel::Warn,
            format!("[Scripts] failed to remove stale script output {error}"),
        );
    }
}

fn select_dependencies(names: &mut IndexMap<String, String>) {
    for (triggers, addition) in [
        (
            &["Creatures:WendigoColossusRaceScript"][..],
            "B21_PlayerFear",
        ),
        (
            &[
                "DefaultKeypadScript",
                "Nuke_MasterScript",
                "EN07_ExternalKeypadAliasScript",
                "Nuke_CodePageRefScript",
                "Nuke_CodesSolutionPrinterScript",
                "Nuke_CodeSolutionMasterScript",
                "EN07_TargetingComputerAliasScript",
                "Nuke_LaunchCardPatrolTerminalScript",
            ][..],
            "B21:KeypadNative",
        ),
        (
            &[
                "ENz04_BotScript",
                "ENz01_AboveScript",
                "ENs02_BlastQuestScript",
            ][..],
            "B21:EnclaveEventSupport",
        ),
        (
            &[
                "W05_MQ_TheWayward_QuestScript",
                "W05_Wayward_SwapMarkerOnCriteria",
                "W05_Wayward_SetAVOnQuestlineComplete",
                "Fragments:Quests:QF_W05_MQ_004P_Crane_0041C976",
            ][..],
            "B21_WaywardState",
        ),
        (
            &[
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2ADB",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2ADC",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2ADD",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2ADE",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2ADF",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2AE0",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2AE1",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2AE2",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2AE3",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2AE4",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2AE5",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2AE6",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2AE7",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2AE8",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2AE9",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2AEA",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2AEB",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2AEC",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2AED",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2AEE",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2AEF",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2AF0",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2AF1",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2AF2",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2AF3",
                "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2AF4",
            ][..],
            "B21:AmmoConverter",
        ),
    ] {
        if triggers
            .iter()
            .any(|n| names.contains_key(&script_patches::key(n)))
        {
            names.insert(script_patches::key(addition), addition.to_owned());
        }
    }
}

fn ancestor_closure(
    ctx: &PhaseCtx<'_>,
    names: &mut IndexMap<String, String>,
    source: &IndexMap<String, (String, PathBuf)>,
    target: &IndexMap<String, Option<PathBuf>>,
) {
    let mut queue = names.values().cloned().collect::<Vec<_>>();
    let mut warned = HashSet::new();
    while let Some(name) = queue.pop() {
        let key = script_patches::key(&name);
        let path = source
            .get(&key)
            .map(|x| &x.1)
            .or_else(|| target.get(&key).and_then(Option::as_ref));
        let Some(path) = path else {
            continue;
        };
        let Ok(pex) = papyrus_core::pex::parse_pex_file(&path.to_string_lossy()) else {
            continue;
        };
        let Some(parent) = pex
            .objects
            .first()
            .map(|o| &o.parent)
            .filter(|p| !p.is_empty())
        else {
            continue;
        };
        let key = script_patches::key(parent);
        if target.contains_key(&key) || names.contains_key(&key) {
            continue;
        }
        if !source.contains_key(&key) {
            if warned.insert(key) {
                log(
                    ctx,
                    LogLevel::Warn,
                    format!(
                        "[Scripts] {name} extends {parent}, which has no PEX in the source tree; leaving unemitted"
                    ),
                );
            }
            continue;
        }
        names.insert(key, parent.clone());
        queue.push(parent.clone());
    }
}

pub fn install_addition(root: &Path, name: &str, path: &Path) -> Result<(), String> {
    let source = read_text(path)?;
    let declared = script_patches::declaration(&source)
        .ok_or_else(|| format!("{} has no Scriptname declaration", path.display()))?;
    if script_patches::key(&declared) != script_patches::key(name) {
        return Err(format!(
            "{} declares {declared}, expected {name}",
            path.display()
        ));
    }
    write_text(
        &root
            .join("Scripts/Source/User")
            .join(relative_path(name, "psc")),
        &source,
    )
}

fn write_text(path: &Path, source: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    #[cfg(windows)]
    let source = source.replace('\n', "\r\n");
    fs::write(path, source.as_bytes()).map_err(|e| e.to_string())
}

fn read_text(path: &Path) -> Result<String, String> {
    fs::read_to_string(path)
        .map(|s| s.replace("\r\n", "\n").replace('\r', "\n"))
        .map_err(|e| e.to_string())
}

fn port_one(
    ctx: &PhaseCtx<'_>,
    name: &str,
    path: &Path,
    patch: Option<&PatchEntry>,
) -> Option<Resolution> {
    let result = (|| {
        let pex = papyrus_core::pex::parse_pex_file(&path.to_string_lossy())?;
        let options = papyrus_core::decompiler::DecompileOptions {
            type_aliases: [
                ("player", "Actor"),
                ("questinstance", "Quest"),
                ("sceneinstance", "Scene"),
                ("region", "Form"),
            ]
            .into_iter()
            .map(|(a, b)| (a.to_owned(), b.to_owned()))
            .collect(),
            drop_script_const: true,
            skip_internal_functions: true,
            fo4_api_compat: true,
        };
        let ast = papyrus_core::decompiler::decompile(&pex, &options)?;
        let source = papyrus_core::emitter::emit_script(&ast);
        script_patches::augment(name, &source)
    })();
    let mut source = match result {
        Ok(source) => source,
        Err(error) => {
            clean(ctx, name);
            return Some(Resolution::new(name, "decompile_failed", None, error));
        }
    };
    if let Some(patch) = patch {
        source = match script_patches::merge(&source, &patch.source) {
            Ok(source) => source,
            Err(error) => return Some(Resolution::new(name, "decompile_failed", None, error)),
        };
        log(
            ctx,
            LogLevel::Info,
            format!("[Scripts] merged fix-folder method patch into {name}"),
        );
    } else if !source
        .lines()
        .any(|line| line.trim_start().to_lowercase().starts_with("event "))
    {
        log(
            ctx,
            LogLevel::Warn,
            format!(
                "[Scripts] {name} decompiled with no logic (FO76 server-side stub); add an original {} to conversion/script_patches/ to implement it",
                relative_path(name, "psc").display()
            ),
        );
    }
    write_text(
        &ctx.mod_path
            .join("Scripts/Source/User")
            .join(relative_path(name, "psc")),
        &source,
    )
    .err()
    .map(|error| Resolution::new(name, "decompile_failed", None, error))
}

fn compile_one(root: &Path, name: &str, imports: &[String], flags: Option<&str>) -> Resolution {
    let psc = root
        .join("Scripts/Source/User")
        .join(relative_path(name, "psc"));
    let output = root.join("data/Scripts").join(relative_path(name, "pex"));
    let result = (|| {
        if output.is_file() {
            fs::remove_file(&output)
                .map_err(|e| format!("could not remove stale output {}: {e}", output.display()))?;
        }
        if !psc.is_file() {
            return Err(format!("missing source {}", psc.display()));
        }
        let source = read_text(&psc)?;
        let compiled = papyrus_core::compiler::compile_source_with_path(
            &source,
            imports,
            papyrus_core::profile::Game::Fo4,
            flags,
            Some(&psc.to_string_lossy()),
        );
        let Some(bytes) = compiled.pex_bytes.filter(|_| compiled.ok) else {
            let messages: Vec<_> = compiled
                .diagnostics
                .iter()
                .take(3)
                .map(|d| format!("{}:{}: {}", d.line, d.col, d.message))
                .collect();
            return Err(if messages.is_empty() {
                "native compiler returned no output".to_owned()
            } else {
                messages.join("; ")
            });
        };
        fs::create_dir_all(output.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::write(&output, bytes).map_err(|e| e.to_string())
    })();
    match result {
        Ok(()) => Resolution::new(name, "compiled", Some(output), ""),
        Err(e) => Resolution::new(name, "compile_failed", None, e),
    }
}

fn compiler_command(
    ctx: &PhaseCtx<'_>,
    resources: &Path,
    name: Option<&str>,
) -> Result<Command, String> {
    let data = ctx
        .target_data_dir
        .ok_or("target data dir not configured")?;
    let compiler = data
        .parent()
        .unwrap_or(data)
        .join("Papyrus Compiler/PapyrusCompiler.exe");
    if !compiler.is_file() {
        return Err(compiler.to_string_lossy().into_owned());
    }
    let source = ctx.mod_path.join("Scripts/Source/User");
    let output = ctx.mod_path.join("data/Scripts");
    let mut imports = vec![source.clone()];
    if let Some(root) = resources.ancestors().nth(4) {
        imports.push(root.join("mods/B21_TalesFromAppalachia/Scripts/Source/User"));
    }
    for path in [
        data.join("Scripts/Source/User"),
        data.join("Scripts/Source/Base"),
    ] {
        if path.is_dir() {
            imports.push(path);
        }
    }
    let flags = data.join("Scripts/Source/Base/Institute_Papyrus_Flags.flg");
    let flags = if flags.is_file() {
        flags.to_string_lossy().into_owned()
    } else {
        "Institute_Papyrus_Flags.flg".to_owned()
    };
    let mut command = Command::new(compiler);
    command.current_dir(&source);
    if let Some(name) = name {
        command.arg(relative_path(name, "").to_string_lossy().replace('\\', "/"));
    } else {
        command.arg(&source).arg("-all");
    }
    command.arg(format!(
        "-import={}",
        imports
            .iter()
            .map(|p| p.to_string_lossy())
            .collect::<Vec<_>>()
            .join(";")
    ));
    command.arg(format!("-output={}", output.display()));
    command.arg("-quiet").arg(format!("-flags={flags}"));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    Ok(command)
}

fn execute_compiler(
    mut command: Command,
    seconds: u64,
    ctx: &PhaseCtx<'_>,
) -> Result<(bool, String), String> {
    let mut output = tempfile::tempfile().map_err(|e| e.to_string())?;
    command.stdout(Stdio::from(output.try_clone().map_err(|e| e.to_string())?));
    command.stderr(Stdio::from(output.try_clone().map_err(|e| e.to_string())?));
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => return Err(error.to_string()),
        }
        if ctx.cancel.load(Ordering::Relaxed) || started.elapsed().as_secs() >= seconds {
            let _ = child.kill();
            let _ = child.wait();
            return Err(if ctx.cancel.load(Ordering::Relaxed) {
                "cancelled"
            } else {
                "compile_timeout"
            }
            .to_owned());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    output.rewind().map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    output.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok((
        status.success(),
        String::from_utf8_lossy(&bytes)
            .replace("\r\n", "\n")
            .trim()
            .to_owned(),
    ))
}

fn compile_exe_one(ctx: &PhaseCtx<'_>, resources: &Path, name: &str) -> Resolution {
    let command = match compiler_command(ctx, resources, Some(name)) {
        Ok(pair) => pair,
        Err(error) => return Resolution::new(name, "compiler_unavailable", None, error),
    };
    let output = ctx
        .mod_path
        .join("data/Scripts")
        .join(relative_path(name, "pex"));
    if let Err(error) = fs::create_dir_all(output.parent().unwrap()) {
        return Resolution::new(name, "compile_failed", None, error.to_string());
    }
    if output.is_file() {
        if let Err(error) = fs::remove_file(&output) {
            return Resolution::new(
                name,
                "compile_failed",
                None,
                format!(
                    "could not remove stale output {}: {error}",
                    output.display()
                ),
            );
        }
    }
    match execute_compiler(command, 60, ctx) {
        Ok((true, _)) if output.is_file() => Resolution::new(name, "compiled", Some(output), ""),
        Ok((_, message)) => Resolution::new(
            name,
            "compile_failed",
            None,
            message.lines().last().unwrap_or("no output").to_owned(),
        ),
        Err(error) if error == "compile_timeout" => {
            Resolution::new(name, "compile_timeout", None, error)
        }
        Err(error) => Resolution::new(name, "compiler_unavailable", None, error),
    }
}

fn compile_exe_batch(ctx: &PhaseCtx<'_>, resources: &Path, names: &[String]) -> Vec<Resolution> {
    let command = match compiler_command(ctx, resources, None) {
        Ok(pair) => pair,
        Err(error) => {
            return names
                .iter()
                .map(|name| Resolution::new(name, "compiler_unavailable", None, &error))
                .collect();
        }
    };
    let output = ctx.mod_path.join("data/Scripts");
    if let Err(error) = fs::create_dir_all(&output) {
        return names
            .iter()
            .map(|name| Resolution::new(name, "compile_failed", None, error.to_string()))
            .collect();
    }
    let mut errors = HashMap::new();
    for name in names {
        let path = output.join(relative_path(name, "pex"));
        if path.is_file() {
            if let Err(error) = fs::remove_file(&path) {
                errors.insert(
                    script_patches::key(name),
                    format!("could not remove stale output {}: {error}", path.display()),
                );
            }
        }
    }
    let execution = execute_compiler(command, 1800 + names.len() as u64 * 2, ctx);
    if let Err(error) = execution {
        return names
            .iter()
            .map(|name| {
                Resolution::new(
                    name,
                    if error == "compile_timeout" {
                        "compile_timeout"
                    } else {
                        "compiler_unavailable"
                    },
                    None,
                    &error,
                )
            })
            .collect();
    }
    for line in execution
        .unwrap()
        .1
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        log(
            ctx,
            LogLevel::Info,
            format!("[compile] {}", line.trim_end()),
        );
    }
    names
        .iter()
        .map(|name| {
            let path = output.join(relative_path(name, "pex"));
            if let Some(error) = errors.get(&script_patches::key(name)) {
                Resolution::new(name, "compile_failed", None, error)
            } else if path.is_file() {
                Resolution::new(name, "compiled", Some(path), "")
            } else {
                Resolution::new(
                    name,
                    "compile_failed",
                    None,
                    "no output (see batch compile log)",
                )
            }
        })
        .collect()
}

pub fn assert_healthy(resolutions: impl IntoIterator<Item = Resolution>) -> Result<(), String> {
    let resolutions: Vec<_> = resolutions.into_iter().collect();
    let unavailable: Vec<_> = resolutions
        .iter()
        .filter(|r| r.status == "compiler_unavailable")
        .collect();
    if let Some(first) = unavailable.first() {
        let detail = if first.message.is_empty() {
            &first.status
        } else {
            &first.message
        };
        return Err(format!(
            "Papyrus toolchain unavailable: {detail}. {} script(s) could not be compiled because the compiler itself could not run. Aborting instead of shipping a plugin with its script bindings stripped.",
            unavailable.len()
        ));
    }
    let attempted = resolutions.iter().filter(|r| r.status != "target").count();
    if attempted > 10 && !resolutions.iter().any(|r| r.status == "compiled") {
        return Err(format!(
            "Papyrus compilation produced no output: 0 of {attempted} script(s) compiled. Aborting instead of shipping a plugin with its script bindings stripped."
        ));
    }
    Ok(())
}

fn assert_delivered(
    patches: &IndexMap<String, PatchEntry>,
    resolutions: &IndexMap<String, Resolution>,
) -> Result<(), String> {
    let mut failures = Vec::new();
    for (key, entry) in patches {
        if !entry.patch.functional() {
            continue;
        }
        match resolutions.get(key) {
            None => failures.push(format!(
                "{}: not selected for script conversion",
                entry.name
            )),
            Some(r) if r.status != "compiled" || r.pex_path.is_none() => failures.push(format!(
                "{}: {}{}",
                entry.name,
                r.status,
                if r.message.is_empty() {
                    String::new()
                } else {
                    format!(": {}", r.message)
                }
            )),
            _ => {}
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "functional Papyrus patch delivery failed for {} script(s): {}",
            failures.len(),
            failures.into_iter().take(10).collect::<Vec<_>>().join("; ")
        ))
    }
}

#[derive(Serialize)]
struct Measurements {
    #[serde(skip)]
    started: Instant,
    events: Vec<Value>,
}
impl Measurements {
    fn record(&mut self, name: &str, start: Instant, fields: Value) {
        let mut value = json!({"name":name,"elapsed_seconds":start.elapsed().as_secs_f64(),"offset_seconds":start.saturating_duration_since(self.started).as_secs_f64(),"scope":"detail","parent":"Convert Scripts"});
        value
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        self.events.push(value);
    }
}

pub struct ConvertScriptsPhase;
impl Phase for ConvertScriptsPhase {
    fn name(&self) -> &'static str {
        PHASE
    }
    fn requires_source_plugin(&self) -> bool {
        false
    }
    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        if ctx.run.source != crate::translator::Game::Fo76
            || ctx.run.target != crate::translator::Game::Fo4
        {
            return Err(PhaseError::BadParams(
                "Convert Scripts supports only fo76:fo4".to_owned(),
            ));
        }
        ctx.check_cancel()?;
        run(ctx).map_err(|error| {
            if error == "cancelled" {
                PhaseError::Cancelled
            } else {
                PhaseError::Internal(error)
            }
        })
    }
}

fn run(ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, String> {
    let started = Instant::now();
    let resources = ctx
        .params
        .get("resources_root")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or("missing resources_root")?;
    let bundled = ctx
        .params
        .get("bundled_corpus_root")
        .and_then(Value::as_str)
        .map(PathBuf::from);
    let output_root = ctx
        .params
        .get("output_root")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| ctx.mod_path.to_owned());
    let diagnostics = ctx
        .params
        .get("diagnostics_root")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| ctx.mod_path.to_owned());
    let source_root = ctx
        .params
        .get("source_data_dir")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let extras: Vec<PathBuf> = ctx
        .params
        .get("additional_source_asset_roots")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(PathBuf::from)
        .collect();
    let mut measurements = Measurements {
        started,
        events: Vec::new(),
    };
    let begin = Instant::now();
    let inspection = inspect(ctx)?;
    let reference_count = inspection.references.len();
    log(
        ctx,
        LogLevel::Info,
        format!(
            "[Scripts] discovered {reference_count} Papyrus script reference(s) across {} record(s)",
            inspection.state.candidate_count()
        ),
    );
    for warning in &inspection.warnings {
        log(ctx, LogLevel::Warn, warning);
    }
    measurements.record("convert_scripts_collect_references",begin,json!({"references":reference_count,"candidate_records":inspection.state.candidate_count()}));
    let begin = Instant::now();
    let (target, authoritative) = target_indexes(ctx);
    measurements.record(
        "convert_scripts_target_index",
        begin,
        json!({"entries":target.len(),"authoritative_entries":authoritative.len()}),
    );
    let begin = Instant::now();
    let roots = source_roots(source_root.as_deref(), &extras);
    let source = pex_index(&roots);
    measurements.record(
        "convert_scripts_source_index",
        begin,
        json!({"roots":roots.len(),"entries":source.len()}),
    );
    let begin = Instant::now();
    let patches = patches(&resources.join("script_patches"))?;
    let addition_names = script_patches::additions("fo76", "fo4");
    let additions: HashMap<_, _> = addition_names
        .iter()
        .map(|n| {
            (
                script_patches::key(n),
                (
                    n,
                    resources
                        .join("script_additions/fo76_fo4")
                        .join(relative_path(n, "psc")),
                ),
            )
        })
        .collect();
    measurements.record("convert_scripts_patch_addition_discovery",begin,json!({"additions":additions.values().filter(|x| x.1.is_file()).count(),"patches":patches.len()}));
    let begin = Instant::now();
    let mut names: IndexMap<_, _> = inspection
        .references
        .iter()
        .map(|r| (script_patches::key(&r.script_name), r.script_name.clone()))
        .collect();
    for (key, (name, _)) in &source {
        if !SKIP.contains(&key.as_str()) {
            names.entry(key.clone()).or_insert_with(|| name.clone());
        }
    }
    log(
        ctx,
        LogLevel::Info,
        format!(
            "[Scripts] including all {} indexed source script(s)",
            source.len()
        ),
    );
    select_dependencies(&mut names);
    ancestor_closure(ctx, &mut names, &source, &authoritative);
    let mut ordered = names.values().cloned().collect::<Vec<_>>();
    ordered.sort_by_key(|n| n.to_lowercase());
    let mut resolutions = IndexMap::new();
    let mut port_names = Vec::new();
    let mut compile_names = Vec::new();
    for name in ordered {
        let key = script_patches::key(&name);
        if let Some((addition, path)) = additions.get(&key) {
            if !path.is_file() {
                resolutions.insert(
                    key,
                    Resolution::new(
                        &name,
                        "addition_missing",
                        None,
                        format!(
                            "required pair-scoped source artifact not found: {}",
                            path.display()
                        ),
                    ),
                );
            } else if let Err(error) = install_addition(ctx.mod_path, addition, path) {
                resolutions.insert(
                    key,
                    Resolution::new(
                        addition,
                        "addition_install_failed",
                        None,
                        format!("could not install full-script addition: {error}"),
                    ),
                );
            } else {
                compile_names.push(addition.to_string());
                log(
                    ctx,
                    LogLevel::Info,
                    format!("[Scripts] installed target-only full script {addition}"),
                );
            }
        } else if SKIP.contains(&key.as_str()) {
            clean(ctx, &name);
            resolutions.insert(key,Resolution::new(&name,"unsupported",None,"FO76 creature specialization has no target PEX; do not replace the FO4/DLC script layer"));
        } else if let Some(path) = authoritative.get(&key) {
            clean(ctx, &name);
            resolutions.insert(key, Resolution::new(&name, "target", path.clone(), ""));
        } else if source.contains_key(&key) {
            port_names.push(name);
        } else if let Some(path) = target.get(&key) {
            clean(ctx, &name);
            resolutions.insert(key, Resolution::new(&name, "target", path.clone(), ""));
        } else {
            clean(ctx, &name);
            resolutions.insert(key, Resolution::new(&name, "source_missing", None, ""));
        }
    }
    measurements.record("convert_scripts_resolution",begin,json!({"requested":names.len(),"source":port_names.len(),"additions":compile_names.len(),"resolved":resolutions.len()}));
    let requested_workers = ctx
        .params
        .get("workers")
        .and_then(Value::as_u64)
        .unwrap_or(1) as usize;
    let workers = requested_workers.max(1).min(port_names.len().max(1));
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .thread_name(|i| format!("script-port-{i}"))
        .build()
        .map_err(|e| e.to_string())?;
    let begin = Instant::now();
    log(
        ctx,
        LogLevel::Info,
        format!(
            "[Scripts] porting {} source script(s) with {workers} worker(s)",
            port_names.len()
        ),
    );
    let results = pool.install(|| {
        port_names
            .par_iter()
            .map(|name| {
                if ctx.cancel.load(Ordering::Relaxed) {
                    return Err("cancelled".to_owned());
                }
                let key = script_patches::key(name);
                Ok((
                    name.clone(),
                    port_one(ctx, name, &source[&key].1, patches.get(&key)),
                ))
            })
            .collect::<Result<Vec<_>, String>>()
    })?;
    for (name, result) in results {
        if let Some(result) = result {
            resolutions.insert(script_patches::key(&name), result);
        } else {
            compile_names.push(name);
        }
    }
    measurements.record(
        "convert_scripts_decompile_port",
        begin,
        json!({"source":port_names.len(),"compile_candidates":compile_names.len()}),
    );
    let begin = Instant::now();
    let selector = ctx
        .params
        .get("compiler")
        .and_then(Value::as_str)
        .unwrap_or("native");
    let results = if compile_names.is_empty() {
        Vec::new()
    } else if selector == "exe-batch" {
        compile_exe_batch(ctx, &resources, &compile_names)
    } else if selector != "native" {
        pool.install(|| {
            compile_names
                .par_iter()
                .map(|name| compile_exe_one(ctx, &resources, name))
                .collect()
        })
    } else {
        let headers = crate::script_headers::prepare_headers(
            ctx.target_extracted_dir,
            ctx.target_data_dir,
            &output_root,
            bundled.as_deref(),
            ctx.run.target_assets.as_deref(),
            "fo4",
        )?;
        for (level, message) in &headers.logs {
            log(
                ctx,
                if level == "WARN" {
                    LogLevel::Warn
                } else {
                    LogLevel::Info
                },
                message,
            );
        }
        if headers.import_roots.is_empty() {
            compile_names.iter().map(|name| Resolution::new(name,"compiler_unavailable",None,"no Fallout 4 script corpus found in the game's loose scripts or its archives; the Papyrus type universe is built from those .pex")).collect()
        } else {
            let imports = dedupe_paths(
                std::iter::once(ctx.mod_path.join("Scripts/Source/User"))
                    .chain(headers.import_roots),
            )
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
            let flags = headers
                .flags_file
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Institute_Papyrus_Flags.flg".to_owned());
            log(
                ctx,
                LogLevel::Info,
                format!(
                    "[Scripts] native-compiling {} script(s)",
                    compile_names.len()
                ),
            );
            pool.install(|| {
                compile_names
                    .par_iter()
                    .map(|name| {
                        if ctx.cancel.load(Ordering::Relaxed) {
                            return Err("cancelled".to_owned());
                        }
                        Ok(compile_one(ctx.mod_path, name, &imports, Some(&flags)))
                    })
                    .collect::<Result<Vec<_>, String>>()
            })?
        }
    };
    if ctx.cancel.load(Ordering::Relaxed) {
        return Err("cancelled".to_owned());
    }
    for mut result in results {
        let name = &result.script_name;
        if let Some(entry) = patches
            .get(&script_patches::key(name))
            .filter(|entry| entry.patch.functional())
        {
            if result.status == "compiled" {
                let path = result.pex_path.as_ref().unwrap();
                let validate = papyrus_core::pex::parse_pex_file(&path.to_string_lossy())
                    .and_then(|pex| script_patches::validate_compiled(&entry.patch, &pex));
                if let Err(error) = validate {
                    result.status = "patch_delivery_failed".to_owned();
                    result.message = error;
                }
            }
        }
        resolutions.insert(script_patches::key(&result.script_name), result);
    }
    measurements.record(
        "convert_scripts_compile",
        begin,
        json!({"candidates":compile_names.len()}),
    );
    let begin = Instant::now();
    assert_healthy(resolutions.values().cloned())?;
    assert_delivered(&patches, &resolutions)?;
    for result in resolutions.values().filter(|r| {
        matches!(
            r.status.as_str(),
            "compile_failed"
                | "compile_timeout"
                | "compiler_unavailable"
                | "addition_install_failed"
                | "addition_missing"
        )
    }) {
        clean(ctx, &result.script_name);
    }
    let evidence = resolutions
        .iter()
        .map(|(key, r)| ScriptResolutionEvidence {
            script_key: key.clone(),
            script_name: r.script_name.clone(),
            status: r.status.clone(),
            pex_path: r
                .pex_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
        })
        .collect();
    let reconciled = script_references::reconcile_script_references(
        &inspection.state,
        evidence,
        |id, subrecords| set_subrecords(ctx.run.target_handle_id, id, subrecords),
    );
    let reconciled = match reconciled {
        Ok(result) => result,
        Err(error) => {
            ctx.run.script_reference_state = Some(inspection.state);
            return Err(error);
        }
    };
    for failure in &reconciled.parse_failures {
        resolutions.insert(
            failure.script_key.clone(),
            Resolution::new(
                &failure.script_name,
                "parse_failed",
                Some(PathBuf::from(&failure.pex_path)),
                &failure.message,
            ),
        );
    }
    for note in &reconciled.unreached_reward_notes {
        log(ctx, LogLevel::Warn, format!("[Scripts] {note}"));
    }
    for note in reconciled.vmad_notes.iter().take(75) {
        log(ctx, LogLevel::Warn, format!("[Scripts] VMAD {note}"));
    }
    if reconciled.vmad_notes.len() > 75 {
        log(
            ctx,
            LogLevel::Warn,
            format!(
                "[Scripts] suppressed {} additional VMAD change logs; the full list is in script_port_report.json",
                reconciled.vmad_notes.len() - 75
            ),
        );
    }
    for note in reconciled.invalid_condition_notes.iter().take(75) {
        log(
            ctx,
            LogLevel::Warn,
            format!("[Scripts] target script variable missing; stripped {note}"),
        );
    }
    if reconciled.invalid_condition_notes.len() > 75 {
        log(
            ctx,
            LogLevel::Warn,
            format!(
                "[Scripts] suppressed {} additional missing-variable logs",
                reconciled.invalid_condition_notes.len() - 75
            ),
        );
    }
    let failed = resolutions.values().filter(|r| !r.ok()).count();
    for r in resolutions.values().filter(|r| !r.ok()).take(75) {
        log(
            ctx,
            LogLevel::Warn,
            format!(
                "[Scripts] {} for {}{}",
                r.status,
                r.script_name,
                if r.message.is_empty() {
                    String::new()
                } else {
                    format!(": {}", r.message)
                }
            ),
        );
    }
    if failed > 75 {
        log(
            ctx,
            LogLevel::Warn,
            format!(
                "[Scripts] suppressed {} additional script failure logs",
                failed - 75
            ),
        );
    }
    let compiled = resolutions
        .values()
        .filter(|r| r.status == "compiled")
        .count();
    let target = resolutions
        .values()
        .filter(|r| r.status == "target")
        .count();
    measurements.record("convert_scripts_reconciliation",begin,json!({"failed":failed,"invalid_conditions":reconciled.invalid_condition_count,"changed_records":reconciled.changed_records,"pex_member_requests":reconciled.pex_member_requests,"pex_member_distinct_paths":reconciled.pex_member_distinct_paths,"pex_member_cache_hits":reconciled.pex_member_cache_hits}));
    let begin = Instant::now();
    let mut ordered: Vec<_> = resolutions.values().collect();
    ordered.sort_by_key(|r| r.script_name.to_lowercase());
    let report = json!({"scripts":ordered,"invalid_conditions":reconciled.invalid_condition_notes,"stripped_conditions":reconciled.stripped_conditions,"stripped_vmad":reconciled.stripped_vmad,"changed_records":reconciled.changed_records,"vmad_changes":reconciled.vmad_notes});
    fs::create_dir_all(&diagnostics).map_err(|e| e.to_string())?;
    write_text(
        &diagnostics.join("script_port_report.json"),
        &serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?,
    )?;
    measurements.record("convert_scripts_report", begin, json!({}));
    measurements.record("convert_scripts", started, json!({"references":reference_count,"scripts":resolutions.len(),"compiled":compiled,"failed":failed,"stripped_conditions":reconciled.stripped_conditions,"stripped_vmad":reconciled.stripped_vmad}));
    write_text(
        &diagnostics.join("script_port_timing.json"),
        &serde_json::to_string_pretty(&measurements).map_err(|e| e.to_string())?,
    )?;
    log(
        ctx,
        LogLevel::Info,
        format!(
            "[Scripts] done: refs={reference_count} scripts={} target={target} compiled={compiled} failed={failed} stripped_conditions={} stripped_vmad={}",
            resolutions.len(),
            reconciled.stripped_conditions,
            reconciled.stripped_vmad
        ),
    );
    Ok(PhaseReport {
        records_changed: reconciled.changed_records as u32,
        assets_written: compiled as u32,
        warnings: (failed + reconciled.invalid_condition_count) as u32,
        items_failed: failed as u32,
        elapsed_ms: started.elapsed().as_millis() as u64,
        ..PhaseReport::default()
    })
}
