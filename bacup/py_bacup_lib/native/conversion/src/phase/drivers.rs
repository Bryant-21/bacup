// Phase: synthesize_drivers
//
// Params shape (JSON):
// {
//   "meshes_subdir": "meshes"    // optional — default "meshes"
// }
//
// Phase output: rewrites behavior HKX files in mod_path/data/<meshes_subdir>/
// with internally-driven telemetry variable chains (hkbDampingModifier +
// hkbModifierGenerator pattern), matching FO4 vanilla weapons (Cryolator,
// Flamer, Minigun).
//
// PhaseReport:
//   assets_written = number of HKX files that had at least one chain injected
//   warnings       = files that could not be unpacked or re-packed
//   records_dropped = total driver chains injected across all files

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value as JsonValue;

use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};

// ---------------------------------------------------------------------------
// Embedded YAML configuration
// ---------------------------------------------------------------------------

const DRIVERS_YAML: &str = include_str!("havok/drivers.yaml");

// ---------------------------------------------------------------------------
// Configuration types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct VariablePattern {
    ramp_events: Vec<String>,
    decay_events: Vec<String>,
    initial_value: f64,
    ramp_target: f64,
    decay_target: f64,
    ramp_damping_kp: f64,
    decay_damping_kp: f64,
}

impl Default for VariablePattern {
    fn default() -> Self {
        Self {
            ramp_events: vec!["WeaponFire".into()],
            decay_events: vec!["WeaponSheathe".into()],
            initial_value: 0.0,
            ramp_target: 1.0,
            decay_target: 0.0,
            ramp_damping_kp: 0.15,
            decay_damping_kp: 0.15,
        }
    }
}

#[derive(Debug)]
struct DriverConfig {
    variable_patterns: HashMap<String, VariablePattern>,
    telemetry_sinks: Vec<String>,
    internal_writer_classes: HashSet<String>,
}

impl DriverConfig {
    fn load(yaml_text: &str) -> Self {
        let raw: serde_json::Value = serde_yaml_to_json(yaml_text);

        let variable_patterns = {
            let mut map = HashMap::new();
            if let Some(patterns) = raw.get("variable_patterns").and_then(|v| v.as_object()) {
                for (name, pat) in patterns {
                    let damping_kp = pat
                        .get("damping_kp")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.15);
                    let p = VariablePattern {
                        ramp_events: string_list_from_value(pat.get("ramp_events")),
                        decay_events: string_list_from_value(pat.get("decay_events")),
                        initial_value: pat
                            .get("initial_value")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0),
                        ramp_target: pat
                            .get("ramp_target")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(1.0),
                        decay_target: pat
                            .get("decay_target")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0),
                        ramp_damping_kp: pat
                            .get("ramp_damping_kp")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(damping_kp),
                        decay_damping_kp: pat
                            .get("decay_damping_kp")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(damping_kp),
                    };
                    map.insert(name.to_lowercase(), p);
                }
            }
            map
        };

        let telemetry_sinks = string_list_from_value(raw.get("telemetry_sinks"));

        let internal_writer_classes: HashSet<String> = {
            let mut s: HashSet<String> = string_list_from_value(raw.get("internal_writer_classes"))
                .into_iter()
                .map(|c| c.to_lowercase())
                .collect();
            // Always include the fallback heuristic: anything containing "modifier"
            // is caught at detection time, but add explicit entries from YAML.
            s.insert("hkbdampingmodifier".into());
            s.insert("bstimermodifier".into());
            s
        };

        Self {
            variable_patterns,
            telemetry_sinks,
            internal_writer_classes,
        }
    }

    fn resolve_pattern(&self, var_name: &str, sink: &str) -> VariablePattern {
        let name_key = var_name.to_lowercase();
        if let Some(p) = self.variable_patterns.get(&name_key) {
            return p.clone();
        }
        let sink_key = sink.split('.').last().unwrap_or(sink).to_lowercase();
        if let Some(p) = self.variable_patterns.get(&sink_key) {
            return p.clone();
        }
        VariablePattern::default()
    }
}

// ---------------------------------------------------------------------------
// Phase impl
// ---------------------------------------------------------------------------

pub struct SynthesizeDriversPhase;

impl Phase for SynthesizeDriversPhase {
    fn name(&self) -> &'static str {
        "synthesize_drivers"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let p = ctx.params;

        let meshes_subdir = p
            .get("meshes_subdir")
            .and_then(|v| v.as_str())
            .unwrap_or("meshes")
            .to_string();

        let meshes_dir = ctx.mod_path.join("data").join(&meshes_subdir);
        let sink = ctx.run.output_sink.clone();
        let data_root = ctx.mod_path.join("data");
        let register_with_sink = |path: &Path| -> bool {
            let Some(s) = &sink else { return true };
            let Ok(rel) = path.strip_prefix(&data_root) else {
                return true;
            };
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            s.add_existing_file(&rel_str, path).is_ok()
        };

        let hkx_files = find_behavior_hkx_files(&meshes_dir);

        let total = hkx_files.len() as u32;

        if hkx_files.is_empty() {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: "synthesize_drivers",
                level: crate::phase::LogLevel::Info,
                message: "No behavior HKX files in mod output — skipping driver synthesis".into(),
            });
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Progress {
                phase: "synthesize_drivers",
                current: 0,
                total: 0,
                item: None,
            });
            return Ok(PhaseReport::default());
        }

        let config = DriverConfig::load(DRIVERS_YAML);

        let mut files_patched: u32 = 0;
        let mut warnings: u32 = 0;
        let mut sink_failures: u32 = 0;
        let mut chains_injected: u32 = 0;

        for (idx, hkx_path) in hkx_files.iter().enumerate() {
            ctx.check_cancel()?;

            let rel = relative_path(ctx.mod_path, hkx_path);

            let _ = ctx.run.event_tx.try_send(PhaseEvent::Progress {
                phase: "synthesize_drivers",
                current: idx as u32,
                total,
                item: Some(rel.clone()),
            });

            // Read HKX bytes
            let hkx_bytes = match std::fs::read(hkx_path) {
                Ok(b) => b,
                Err(e) => {
                    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                        phase: "synthesize_drivers",
                        level: crate::phase::LogLevel::Warn,
                        message: format!("[BehaviorDriver] Could not read {rel}: {e}"),
                    });
                    warnings += 1;
                    continue;
                }
            };

            // Unpack to XML
            let xml = match havok_native::api::havok_hkx_to_xml(&hkx_bytes) {
                Ok(x) => x,
                Err(e) => {
                    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                        phase: "synthesize_drivers",
                        level: crate::phase::LogLevel::Warn,
                        message: format!("[BehaviorDriver] Could not unpack {rel}: {e}"),
                    });
                    warnings += 1;
                    continue;
                }
            };

            // Detect unbound variables
            let detections = match detect_unbound_variables(&xml, &config) {
                Ok(d) => d,
                Err(e) => {
                    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                        phase: "synthesize_drivers",
                        level: crate::phase::LogLevel::Warn,
                        message: format!("[BehaviorDriver] Detection failed for {rel}: {e}"),
                    });
                    warnings += 1;
                    continue;
                }
            };

            if detections.is_empty() {
                continue;
            }

            // Inject all chains into the XML
            let behavior_name = behavior_name_from_rel_path(&rel);
            let mut current_xml = xml;
            let mut chain_failed = false;

            for (var_name, sink) in &detections {
                let pattern = config.resolve_pattern(var_name, sink);
                match inject_driver_chain(&current_xml, var_name, sink, &behavior_name, &pattern) {
                    Ok(new_xml) => {
                        current_xml = new_xml;
                    }
                    Err(e) => {
                        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                            phase: "synthesize_drivers",
                            level: crate::phase::LogLevel::Error,
                            message: format!(
                                "[BehaviorDriver] Injection failed for {behavior_name}.{var_name}: {e}"
                            ),
                        });
                        chain_failed = true;
                        break;
                    }
                }
            }

            if chain_failed {
                warnings += 1;
                continue;
            }

            // Repack XML → HKX bytes
            let new_hkx = match havok_native::api::havok_xml_to_hkx(&current_xml) {
                Ok(b) => b,
                Err(e) => {
                    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                        phase: "synthesize_drivers",
                        level: crate::phase::LogLevel::Error,
                        message: format!("[BehaviorDriver] Repack failed for {rel}: {e}"),
                    });
                    warnings += 1;
                    continue;
                }
            };

            // Write back to disk
            if let Err(e) = std::fs::write(hkx_path, &new_hkx) {
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: "synthesize_drivers",
                    level: crate::phase::LogLevel::Error,
                    message: format!("[BehaviorDriver] Write failed for {rel}: {e}"),
                });
                warnings += 1;
                continue;
            }

            if !register_with_sink(hkx_path) {
                sink_failures += 1;
            }

            files_patched += 1;
            chains_injected += detections.len() as u32;

            for (var_name, _) in &detections {
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: "synthesize_drivers",
                    level: crate::phase::LogLevel::Info,
                    message: format!(
                        "[BehaviorDriver] Injected damper for {behavior_name}.{var_name} -> {rel}"
                    ),
                });
            }
        }

        let _ = ctx.run.event_tx.try_send(PhaseEvent::Progress {
            phase: "synthesize_drivers",
            current: total,
            total,
            item: None,
        });

        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "synthesize_drivers",
            level: crate::phase::LogLevel::Info,
            message: format!(
                "Behavior driver synthesis: {chains_injected} injection(s) across \
                 {files_patched} file(s), {warnings} skipped, {total} HKX file(s) scanned"
            ),
        });

        Ok(PhaseReport {
            assets_written: files_patched,
            warnings,
            items_failed: warnings + sink_failures,
            // Reuse records_dropped to count total chains injected
            records_dropped: chains_injected,
            ..Default::default()
        })
    }
}

// ---------------------------------------------------------------------------
// File-system helpers
// ---------------------------------------------------------------------------

fn find_behavior_hkx_files(meshes_dir: &Path) -> Vec<PathBuf> {
    if !meshes_dir.is_dir() {
        return Vec::new();
    }
    let mut found = Vec::new();
    visit_dir_for_behaviors(meshes_dir, &mut found);
    found.sort();
    found
}

fn visit_dir_for_behaviors(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            visit_dir_for_behaviors(&path, out);
        } else if path
            .extension()
            .map_or(false, |e| e.eq_ignore_ascii_case("hkx"))
        {
            // Include only if "behaviors" or "uniquebehaviors" appears in the path
            let path_lower = path.to_string_lossy().to_lowercase();
            if path_lower.contains("behaviors") || path_lower.contains("uniquebehaviors") {
                out.push(path);
            }
        }
    }
}

fn relative_path(base: &Path, abs: &Path) -> String {
    abs.strip_prefix(base)
        .unwrap_or(abs)
        .to_string_lossy()
        .replace('\\', "/")
}

fn behavior_name_from_rel_path(rel_path: &str) -> String {
    let normalized = rel_path.replace('\\', "/");
    let parts: Vec<&str> = normalized.split('/').collect();
    // If "UniqueBehaviors" is in the path, take the folder after it
    if let Some(idx) = parts
        .iter()
        .position(|p| p.eq_ignore_ascii_case("UniqueBehaviors"))
    {
        if idx + 1 < parts.len() {
            return parts[idx + 1].to_string();
        }
    }
    // Otherwise use the grandparent folder name (3rd from end)
    if parts.len() >= 3 {
        return parts[parts.len() - 3].to_string();
    }
    // Fallback: stem of the file
    parts
        .last()
        .and_then(|s| s.rsplit('.').nth(1).or(Some(s)))
        .unwrap_or("unknown")
        .to_string()
}

// ---------------------------------------------------------------------------
// XML detection (read-only via roxmltree)
// ---------------------------------------------------------------------------

/// Returns (variable_name, sink_member_path) pairs for variables that are:
/// - declared in hkbBehaviorGraphStringData.variableNames
/// - bound (via hkbVariableBindingSet) to a memberPath matching a telemetry sink
/// - NOT written by any modifier-class object
fn detect_unbound_variables(
    xml: &str,
    config: &DriverConfig,
) -> Result<Vec<(String, String)>, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("XML parse: {e}"))?;

    let sinks: HashSet<String> = config
        .telemetry_sinks
        .iter()
        .map(|s| s.to_lowercase())
        .collect();

    let writer_classes: &HashSet<String> = &config.internal_writer_classes;

    // Collect all hkobjects indexed by their name attribute
    let mut objects_by_name: HashMap<&str, roxmltree::Node> = HashMap::new();
    for node in doc.descendants() {
        if node.tag_name().name() == "hkobject" {
            if let Some(name) = node.attribute("name") {
                objects_by_name.insert(name, node);
            }
        }
    }

    // Find hkbBehaviorGraphStringData to get variable names
    let mut variable_names: Vec<String> = Vec::new();
    for node in doc.descendants() {
        if node.tag_name().name() == "hkobject"
            && node.attribute("class") == Some("hkbBehaviorGraphStringData")
        {
            variable_names = read_hkcstrings(&node, "variableNames");
            break;
        }
    }
    if variable_names.is_empty() {
        return Ok(Vec::new());
    }

    // For each hkobject: collect binding info and detect writer classes
    let mut sink_bindings: HashMap<usize, String> = HashMap::new(); // var_index -> member_path
    let mut written_indices: HashSet<usize> = HashSet::new();

    for node in doc.descendants() {
        if node.tag_name().name() != "hkobject" {
            continue;
        }
        let cls = node.attribute("class").unwrap_or("").to_lowercase();

        // Check for binding set reference
        let binding_ref = read_hkparam_text(&node, "variableBindingSet");
        if !binding_ref.is_empty() && binding_ref != "null" {
            if let Some(bs_node) = objects_by_name.get(binding_ref.as_str()) {
                let pairs = read_binding_pairs(bs_node);
                for (member_path, vi) in &pairs {
                    if *vi < variable_names.len() {
                        let simple = member_path
                            .split('.')
                            .last()
                            .unwrap_or(member_path)
                            .split('/')
                            .last()
                            .unwrap_or(member_path)
                            .to_lowercase();
                        if sinks.contains(&simple) {
                            sink_bindings
                                .entry(*vi)
                                .or_insert_with(|| member_path.clone());
                        }
                    }
                }

                // If this node is a writer class, record the variable indices it touches
                let is_writer = writer_classes.contains(&cls)
                    || (cls.contains("modifier") && cls != "hkbvariablebindingset");
                if is_writer {
                    for (_, vi) in &pairs {
                        if *vi < variable_names.len() {
                            written_indices.insert(*vi);
                        }
                    }
                }
            }
        }
    }

    let mut results = Vec::new();
    let mut seen_vars = HashSet::new();
    for (vi, member_path) in &sink_bindings {
        if written_indices.contains(vi) {
            continue;
        }
        let var_name = variable_names[*vi].clone();
        if seen_vars.insert(var_name.clone()) {
            results.push((var_name, member_path.clone()));
        }
    }
    Ok(results)
}

fn read_hkcstrings(node: &roxmltree::Node, param_name: &str) -> Vec<String> {
    for child in node.children() {
        if child.tag_name().name() == "hkparam" && child.attribute("name") == Some(param_name) {
            return child
                .children()
                .filter(|n| n.tag_name().name() == "hkcstring")
                .map(|n| n.text().unwrap_or("").trim().to_string())
                .collect();
        }
    }
    Vec::new()
}

fn read_hkparam_text(node: &roxmltree::Node, param_name: &str) -> String {
    for child in node.children() {
        if child.tag_name().name() == "hkparam" && child.attribute("name") == Some(param_name) {
            return child.text().unwrap_or("").trim().to_string();
        }
    }
    String::new()
}

fn read_binding_pairs(bs_node: &roxmltree::Node) -> Vec<(String, usize)> {
    let mut pairs = Vec::new();
    for child in bs_node.children() {
        if child.tag_name().name() != "hkparam" || child.attribute("name") != Some("bindings") {
            continue;
        }
        for binding in child.children() {
            if binding.tag_name().name() != "hkobject" {
                continue;
            }
            let mp = read_hkparam_text(&binding, "memberPath");
            let vi_str = read_hkparam_text(&binding, "variableIndex");
            if let Ok(vi) = vi_str.parse::<isize>() {
                if vi >= 0 && !mp.is_empty() {
                    pairs.push((mp, vi as usize));
                }
            }
        }
    }
    pairs
}

// ---------------------------------------------------------------------------
// XML injection (string-based mutation)
// ---------------------------------------------------------------------------

/// Inject the full driver chain into the XML string. Returns the modified XML.
///
/// Strategy (matches vanilla FO4 Cryolator / Flamer / Minigun):
/// 1. Add helper variables <var>_Raw + <var>_DampRate to behavior graph data
/// 2. Build hkbDampingModifier + hkbModifierGenerator wrapping host state's generator
/// 3. Build inner 2-state hkbStateMachine (IdleState / FiringState), keeping the
///    damper active in both states so the driven value can decay while idle
/// 4. Re-point host state's generator at the inner state machine
/// 5. Append all new objects to the <hksection name="__data__"> section
///
/// Idempotent: if a hkbDampingModifier already writes variable_name, skip.
fn inject_driver_chain(
    xml: &str,
    var_name: &str,
    _sink: &str,
    _behavior_name: &str,
    pattern: &VariablePattern,
) -> Result<String, String> {
    // Quick idempotency check: if a hkbDampingModifier exists and its dampedValue
    // binding references the same variable name, skip.
    if is_already_driven(xml, var_name) {
        return Ok(xml.to_string());
    }

    let var_names = extract_variable_names(xml)?;
    let target_var_index = var_names
        .iter()
        .position(|n| n == var_name)
        .ok_or_else(|| format!("variable {var_name:?} not declared in this behavior"))?;

    // Find the maximum existing object ID (#NNNN) to allocate new ones above it
    let max_id = find_max_object_id(xml);
    let mut id_counter = max_id + 1;
    let mut next_id = || -> String {
        let id = format!("#{id_counter:04}");
        id_counter += 1;
        id
    };

    // New variable indices
    let raw_var_index = var_names.len();
    let damp_rate_var_index = var_names.len() + 1;

    // Append the two helper variables to the behavior data structures
    let mut xml_mut = append_variables(
        xml,
        &[
            (format!("{var_name}_Raw"), pattern.decay_target),
            (format!("{var_name}_DampRate"), pattern.decay_damping_kp),
        ],
    )?;

    // Resolve or append event IDs
    let (xml_after_events, ramp_event_ids) =
        resolve_or_append_events(&xml_mut, &pattern.ramp_events)?;
    xml_mut = xml_after_events;
    let (xml_after_events2, decay_event_ids) =
        resolve_or_append_events(&xml_mut, &pattern.decay_events)?;
    xml_mut = xml_after_events2;

    // Extend eventInfos to match event name count
    xml_mut = extend_event_infos(&xml_mut)?;

    // Find the host state's generator reference and state id
    let (host_gen_ref, host_state_id_in_sm) = find_host_state_generator(&xml_mut)?;

    // Allocate IDs for all new objects
    let damp_bs_id = next_id();
    let damp_mod_id = next_id();
    let damper_gen_id = next_id();
    let idle_bs_id = next_id();
    let idle_assign_id = next_id();
    let idle_mod_gen_id = next_id();
    let fire_bs_id = next_id();
    let fire_assign_id = next_id();
    let fire_mod_gen_id = next_id();
    let transition_eff_id = next_id();
    let idle_trans_arr_id = next_id();
    let fire_trans_arr_id = next_id();
    let idle_state_info_id = next_id();
    let fire_state_info_id = next_id();
    let inner_sm_id = next_id();

    // Build the new XML objects
    let mut new_objects = String::new();

    // 1. Damping binding set
    new_objects += &build_binding_set(
        &damp_bs_id,
        &[
            ("kP", damp_rate_var_index),
            ("rawValue", raw_var_index),
            ("dampedValue", target_var_index),
        ],
    );

    // 2. DampingModifier
    new_objects += &build_damping_modifier(
        &damp_mod_id,
        &format!("{var_name}_DampingModifier"),
        pattern.decay_damping_kp,
        &damp_bs_id,
    );

    // 3. DamperGen (wraps original host generator)
    new_objects += &build_modifier_generator(
        &damper_gen_id,
        &format!("{var_name}_DamperGen"),
        &damp_mod_id,
        &host_gen_ref,
    );

    // 4. IdleState: write the decay target/rate while keeping the damper active.
    new_objects += &build_binding_set(
        &idle_bs_id,
        &[
            ("floatVariable1", raw_var_index),
            ("floatVariable2", damp_rate_var_index),
        ],
    );
    new_objects += &build_assign_variables_modifier(
        &idle_assign_id,
        &format!("{var_name}_AssignIdle"),
        &idle_bs_id,
        &[pattern.decay_target, pattern.decay_damping_kp],
    );
    new_objects += &build_modifier_generator(
        &idle_mod_gen_id,
        &format!("{var_name}_IdleModGen"),
        &idle_assign_id,
        &damper_gen_id,
    );

    // 5. FiringState: BSAssignVariablesModifier (writes ramp_target) + DamperGen
    new_objects += &build_binding_set(
        &fire_bs_id,
        &[
            ("floatVariable1", raw_var_index),
            ("floatVariable2", damp_rate_var_index),
        ],
    );
    new_objects += &build_assign_variables_modifier(
        &fire_assign_id,
        &format!("{var_name}_AssignFire"),
        &fire_bs_id,
        &[pattern.ramp_target, pattern.ramp_damping_kp],
    );
    new_objects += &build_modifier_generator(
        &fire_mod_gen_id,
        &format!("{var_name}_FireModGen"),
        &fire_assign_id,
        &damper_gen_id,
    );

    // 6. Zero-duration transition effect
    new_objects += &build_blending_transition_effect(
        &transition_eff_id,
        &format!("{var_name}_SnapTransition"),
    );

    // 7. Transition arrays
    let idle_transitions: Vec<(usize, usize)> =
        ramp_event_ids.iter().map(|&eid| (eid, 1usize)).collect();
    new_objects +=
        &build_transition_info_array(&idle_trans_arr_id, &idle_transitions, &transition_eff_id);

    let fire_transitions: Vec<(usize, usize)> =
        decay_event_ids.iter().map(|&eid| (eid, 0usize)).collect();
    new_objects +=
        &build_transition_info_array(&fire_trans_arr_id, &fire_transitions, &transition_eff_id);

    // 8. State info objects
    new_objects += &build_state_machine_state_info(
        &idle_state_info_id,
        &format!("{var_name}_IdleState"),
        0,
        &idle_mod_gen_id,
        &idle_trans_arr_id,
    );
    new_objects += &build_state_machine_state_info(
        &fire_state_info_id,
        &format!("{var_name}_FiringState"),
        1,
        &fire_mod_gen_id,
        &fire_trans_arr_id,
    );

    // 9. Inner state machine
    new_objects += &build_inner_state_machine(
        &inner_sm_id,
        &format!("{var_name}_DriverStateMachine"),
        0,
        &[idle_state_info_id.clone(), fire_state_info_id.clone()],
    );

    // Re-point host state's generator at the inner state machine
    xml_mut =
        replace_host_state_generator(&xml_mut, &host_gen_ref, host_state_id_in_sm, &inner_sm_id)?;

    // Append all new objects before </hksection>
    xml_mut = append_to_data_section(&xml_mut, &new_objects)?;

    Ok(xml_mut)
}

// ---------------------------------------------------------------------------
// XML string helpers
// ---------------------------------------------------------------------------

fn fmt_float(v: f64) -> String {
    format!("{v:.6}")
}

fn is_already_driven(xml: &str, var_name: &str) -> bool {
    // Quick string check: if a DampingModifier referencing var_name exists
    xml.contains("hkbDampingModifier") && xml.contains(&format!("{var_name}_DampingModifier"))
}

fn extract_variable_names(xml: &str) -> Result<Vec<String>, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("XML parse: {e}"))?;
    for node in doc.descendants() {
        if node.tag_name().name() == "hkobject"
            && node.attribute("class") == Some("hkbBehaviorGraphStringData")
        {
            return Ok(read_hkcstrings(&node, "variableNames"));
        }
    }
    Ok(Vec::new())
}

fn find_max_object_id(xml: &str) -> usize {
    let mut max: usize = 0;
    let doc = match roxmltree::Document::parse(xml) {
        Ok(d) => d,
        Err(_) => return 1000,
    };
    for node in doc.descendants() {
        if node.tag_name().name() == "hkobject" {
            if let Some(name) = node.attribute("name") {
                if let Some(stripped) = name.strip_prefix('#') {
                    if let Ok(n) = stripped.parse::<usize>() {
                        if n > max {
                            max = n;
                        }
                    }
                }
            }
        }
    }
    max
}

/// Append helper variables to the behavior graph data.
/// Mutates: variableInfos, variableNames (in stringData), wordVariableValues.
fn append_variables(xml: &str, vars: &[(String, f64)]) -> Result<String, String> {
    let mut result = xml.to_string();
    for (name, initial_value) in vars {
        result = append_one_variable(&result, name, *initial_value)?;
    }
    Ok(result)
}

fn append_one_variable(xml: &str, var_name: &str, initial_value: f64) -> Result<String, String> {
    // 1. Append a variableInfo entry
    let var_info_block = format!(
        r#"
                            <hkobject class="hkbVariableInfo" signature="0xa5ae6be2">
                                <hkparam name="role">
                                    <hkobject class="hkbRoleAttribute" signature="0xfecef669">
                                        <hkparam name="role">ROLE_DEFAULT</hkparam>
                                        <hkparam name="flags">FLAG_NONE</hkparam>
                                    </hkobject>
                                </hkparam>
                                <hkparam name="type">VARIABLE_TYPE_REAL</hkparam>
                            </hkobject>"#
    );

    // Insert before closing tag of variableInfos param.
    // Strategy: find numelements in variableInfos, increment it, append the new entry.
    let result = insert_into_numelements_param(xml, "variableInfos", &var_info_block, 1)?;

    // 2. Append the variable name as hkcstring
    let cstring = format!("\n                    <hkcstring>{var_name}</hkcstring>");
    let result = insert_into_numelements_param(&result, "variableNames", &cstring, 1)?;

    // 3. Append the initial value as a word variable (IEEE 754 bits)
    let raw_bits = initial_value.to_f32_bits();
    let word_entry = format!(
        r#"
                    <hkobject class="hkbVariableValue" signature="0x0b99bd6a">
                        <hkparam name="value">{raw_bits}</hkparam>
                    </hkobject>"#
    );
    let result = insert_into_numelements_param(&result, "wordVariableValues", &word_entry, 1)?;

    Ok(result)
}

trait ToF32Bits {
    fn to_f32_bits(self) -> u32;
}

impl ToF32Bits for f64 {
    fn to_f32_bits(self) -> u32 {
        (self as f32).to_bits()
    }
}

fn resolve_or_append_events(
    xml: &str,
    event_names: &[String],
) -> Result<(String, Vec<usize>), String> {
    let existing = extract_event_names(xml)?;
    let lower_index: HashMap<String, usize> = existing
        .iter()
        .enumerate()
        .map(|(i, n)| (n.to_lowercase(), i))
        .collect();

    let mut ids = Vec::new();
    let mut to_append = Vec::new();

    for name in event_names {
        let key = name.to_lowercase();
        if let Some(&idx) = lower_index.get(&key) {
            ids.push(idx);
        } else {
            ids.push(existing.len() + to_append.len());
            to_append.push(name.clone());
        }
    }

    let mut result = xml.to_string();
    for name in &to_append {
        let cstring = format!("\n                    <hkcstring>{name}</hkcstring>");
        result = insert_into_numelements_param(&result, "eventNames", &cstring, 1)?;
    }

    Ok((result, ids))
}

fn extract_event_names(xml: &str) -> Result<Vec<String>, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("XML parse: {e}"))?;
    for node in doc.descendants() {
        if node.tag_name().name() == "hkobject"
            && node.attribute("class") == Some("hkbBehaviorGraphStringData")
        {
            return Ok(read_hkcstrings(&node, "eventNames"));
        }
    }
    Ok(Vec::new())
}

fn extend_event_infos(xml: &str) -> Result<String, String> {
    let event_count = extract_event_names(xml)?.len();
    let info_count = count_event_infos(xml)?;
    let to_add = event_count.saturating_sub(info_count);
    let mut result = xml.to_string();
    for _ in 0..to_add {
        let entry = r#"
                            <hkobject class="hkbEventInfo" signature="0x5874eed4">
                                <hkparam name="flags">0</hkparam>
                            </hkobject>"#;
        result = insert_into_numelements_param(&result, "eventInfos", entry, 1)?;
    }
    Ok(result)
}

fn count_event_infos(xml: &str) -> Result<usize, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("XML parse: {e}"))?;
    for node in doc.descendants() {
        if node.tag_name().name() == "hkobject"
            && node.attribute("class") == Some("hkbBehaviorGraphData")
        {
            for child in node.children() {
                if child.tag_name().name() == "hkparam"
                    && child.attribute("name") == Some("eventInfos")
                {
                    return Ok(child
                        .children()
                        .filter(|n| n.tag_name().name() == "hkobject")
                        .count());
                }
            }
        }
    }
    Ok(0)
}

/// Find the generator reference in the first hkbStateMachineStateInfo's generator param,
/// and the stateId within that state.
fn find_host_state_generator(xml: &str) -> Result<(String, usize), String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("XML parse: {e}"))?;

    // Follow: hkbBehaviorGraph.rootGenerator → hkbStateMachine.states[0] → hkbStateMachineStateInfo
    let graph = doc
        .descendants()
        .find(|n| {
            n.tag_name().name() == "hkobject" && n.attribute("class") == Some("hkbBehaviorGraph")
        })
        .ok_or("no hkbBehaviorGraph")?;

    let root_gen_ref = read_hkparam_text(&graph, "rootGenerator");

    let mut objects_by_name: HashMap<&str, roxmltree::Node> = HashMap::new();
    for node in doc.descendants() {
        if node.tag_name().name() == "hkobject" {
            if let Some(name) = node.attribute("name") {
                objects_by_name.insert(name, node);
            }
        }
    }

    // Find a hkbStateMachineStateInfo by traversing from root
    let state_info = find_first_state_info(&objects_by_name, &root_gen_ref);

    if let Some(state) = state_info {
        let gen_ref = read_hkparam_text(&state, "generator");
        let state_id_str = read_hkparam_text(&state, "stateId");
        let state_id = state_id_str.parse::<usize>().unwrap_or(0);
        if !gen_ref.is_empty() && gen_ref != "null" {
            return Ok((gen_ref, state_id));
        }
    }

    // Fallback: find any state info in the doc
    for node in doc.descendants() {
        if node.tag_name().name() == "hkobject"
            && node.attribute("class") == Some("hkbStateMachineStateInfo")
        {
            let gen_ref = read_hkparam_text(&node, "generator");
            let state_id_str = read_hkparam_text(&node, "stateId");
            let state_id = state_id_str.parse::<usize>().unwrap_or(0);
            if !gen_ref.is_empty() && gen_ref != "null" {
                return Ok((gen_ref, state_id));
            }
        }
    }

    Err("could not find a host state with a generator".into())
}

fn find_first_state_info<'a>(
    objects_by_name: &HashMap<&str, roxmltree::Node<'a, 'a>>,
    root_gen_ref: &str,
) -> Option<roxmltree::Node<'a, 'a>> {
    let mut current_ref = root_gen_ref.to_string();
    let mut visited = HashSet::new();
    loop {
        if visited.contains(&current_ref) {
            break;
        }
        visited.insert(current_ref.clone());
        let node = objects_by_name.get(current_ref.as_str())?;
        let cls = node.attribute("class").unwrap_or("");
        if cls == "hkbStateMachineStateInfo" {
            return Some(*node);
        }
        if cls == "hkbStateMachine" {
            // Get first state reference
            for child in node.children() {
                if child.tag_name().name() == "hkparam" && child.attribute("name") == Some("states")
                {
                    for state_ref_el in child.children() {
                        if state_ref_el.tag_name().name() == "hkobject" {
                            let text = state_ref_el.text().unwrap_or("").trim();
                            if !text.is_empty() {
                                current_ref = text.to_string();
                                break;
                            }
                        }
                    }
                    break;
                }
            }
        } else {
            break;
        }
    }
    None
}

/// Replace the host state's generator reference with the inner state machine id.
/// The host state is identified by its current generator reference.
fn replace_host_state_generator(
    xml: &str,
    original_gen_ref: &str,
    _host_state_id: usize,
    new_gen_ref: &str,
) -> Result<String, String> {
    // We need to find the hkbStateMachineStateInfo that references original_gen_ref
    // as its generator param, and replace that reference.
    // Strategy: find the pattern '<hkparam name="generator">ORIG</hkparam>' within
    // an hkbStateMachineStateInfo block, and replace it.
    let search = format!(r#"<hkparam name="generator">{original_gen_ref}</hkparam>"#);
    let replace = format!(r#"<hkparam name="generator">{new_gen_ref}</hkparam>"#);

    if xml.contains(&search) {
        Ok(xml.replacen(&search, &replace, 1))
    } else {
        // Fallback: replace just the text node value within a generator param
        Err(format!(
            "could not find generator reference {original_gen_ref:?} in state info"
        ))
    }
}

fn append_to_data_section(xml: &str, new_objects: &str) -> Result<String, String> {
    // Find </hksection> and insert before it
    const END_TAG: &str = "</hksection>";
    let pos = xml.rfind(END_TAG).ok_or("no </hksection> tag in XML")?;
    let mut result = xml.to_string();
    result.insert_str(pos, new_objects);
    Ok(result)
}

/// Insert `content` into a named `<hkparam numelements="N">` block just before its close,
/// and increment the numelements counter by `count`.
fn insert_into_numelements_param(
    xml: &str,
    param_name: &str,
    content: &str,
    count: usize,
) -> Result<String, String> {
    // Pattern: <hkparam name="PARAM" numelements="N">
    let search_open = format!(r#"<hkparam name="{param_name}" numelements=""#);
    let pos = xml
        .find(&search_open)
        .ok_or_else(|| format!("param {param_name:?} not found"))?;

    // Find the numelements value and the closing quote
    let after = &xml[pos + search_open.len()..];
    let end_quote = after
        .find('"')
        .ok_or_else(|| format!("malformed numelements for {param_name}"))?;
    let num_str = &after[..end_quote];
    let num: usize = num_str
        .parse()
        .map_err(|e| format!("parse numelements for {param_name}: {e}"))?;
    let new_num = num + count;

    // Find the closing </hkparam> for this param block
    // We need to handle nesting, but for simple params this is the first </hkparam>
    // after the opening. For complex nested params (like variableInfos that contain
    // hkobjects), we need to find the matching close.
    // Use a depth counter approach.
    let open_end_in_after = after.find('>').ok_or("malformed hkparam")?;
    let param_body_start =
        pos + search_open.len() + end_quote + 1 + (open_end_in_after - end_quote);

    let param_body = &xml[param_body_start..];
    let close_pos = find_hkparam_close(param_body)
        .ok_or_else(|| format!("no closing </hkparam> for {param_name}"))?;

    let insert_pos = param_body_start + close_pos;

    let mut result = xml.to_string();
    // Replace numelements value
    let num_start = pos + search_open.len();
    let num_end = num_start + end_quote;
    result.replace_range(num_start..num_end, &new_num.to_string());

    // Recalculate insert position after the string length change
    let len_diff = new_num.to_string().len() as isize - num_str.len() as isize;
    let adjusted_insert = (insert_pos as isize + len_diff) as usize;
    result.insert_str(adjusted_insert, content);

    Ok(result)
}

fn find_hkparam_close(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut i = 0;
    while i < s.len() {
        if s[i..].starts_with("<hkparam") {
            depth += 1;
            i += 8;
        } else if s[i..].starts_with("</hkparam>") {
            if depth == 0 {
                return Some(i);
            }
            depth -= 1;
            i += 10;
        } else {
            i += 1;
        }
    }
    None
}

// ---------------------------------------------------------------------------
// HKX XML object builders
// ---------------------------------------------------------------------------

fn build_binding_set(id: &str, bindings: &[(&str, usize)]) -> String {
    let count = bindings.len();
    let mut binding_rows = String::new();
    for (member_path, vi) in bindings {
        binding_rows += &format!(
            r#"
                    <hkobject class="hkbVariableBindingSetBinding" signature="0x4d592f72">
                        <hkparam name="memberPath">{member_path}</hkparam>
                        <hkparam name="memberClass">null</hkparam>
                        <hkparam name="offsetInObjectPlusOne">0</hkparam>
                        <hkparam name="offsetInArrayPlusOne">0</hkparam>
                        <hkparam name="rootVariableIndex">0</hkparam>
                        <hkparam name="variableIndex">{vi}</hkparam>
                        <hkparam name="bitIndex">-1</hkparam>
                        <hkparam name="bindingType">BINDING_TYPE_VARIABLE</hkparam>
                        <hkparam name="memberType">0</hkparam>
                        <hkparam name="variableType">0</hkparam>
                        <hkparam name="flags">0</hkparam>
                    </hkobject>"#
        );
    }
    format!(
        r#"
                <hkobject name="{id}" class="hkbVariableBindingSet" signature="0xe942f339">
                    <hkparam name="bindings" numelements="{count}">{binding_rows}
                    </hkparam>
                    <hkparam name="indexOfBindingToEnable">-1</hkparam>
                </hkobject>"#
    )
}

fn build_damping_modifier(id: &str, name: &str, kp: f64, binding_set_ref: &str) -> String {
    let kp_s = fmt_float(kp);
    format!(
        r#"
                <hkobject name="{id}" class="hkbDampingModifier" signature="0x68a51d05">
                    <hkparam name="variableBindingSet">{binding_set_ref}</hkparam>
                    <hkparam name="userData">1</hkparam>
                    <hkparam name="name">{name}</hkparam>
                    <hkparam name="enable">true</hkparam>
                    <hkparam name="kP">{kp_s}</hkparam>
                    <hkparam name="kI">0.000000</hkparam>
                    <hkparam name="kD">0.000000</hkparam>
                    <hkparam name="enableScalarDamping">true</hkparam>
                    <hkparam name="enableVectorDamping">false</hkparam>
                    <hkparam name="rawValue">0.000000</hkparam>
                    <hkparam name="dampedValue">0.000000</hkparam>
                    <hkparam name="rawVector">(0.000000 0.000000 0.000000 0.000000)</hkparam>
                    <hkparam name="dampedVector">(0.000000 0.000000 0.000000 0.000000)</hkparam>
                    <hkparam name="vecErrorSum">(0.000000 0.000000 0.000000 0.000000)</hkparam>
                    <hkparam name="vecPreviousError">(0.000000 0.000000 0.000000 0.000000)</hkparam>
                    <hkparam name="errorSum">0.000000</hkparam>
                    <hkparam name="previousError">0.000000</hkparam>
                </hkobject>"#
    )
}

fn build_modifier_generator(
    id: &str,
    name: &str,
    modifier_ref: &str,
    inner_gen_ref: &str,
) -> String {
    format!(
        r#"
                <hkobject name="{id}" class="hkbModifierGenerator" signature="0xc499fc9e">
                    <hkparam name="variableBindingSet">null</hkparam>
                    <hkparam name="userData">1</hkparam>
                    <hkparam name="name">{name}</hkparam>
                    <hkparam name="modifier">{modifier_ref}</hkparam>
                    <hkparam name="generator">{inner_gen_ref}</hkparam>
                </hkobject>"#
    )
}

fn build_assign_variables_modifier(
    id: &str,
    name: &str,
    binding_set_ref: &str,
    float_values: &[f64],
) -> String {
    let mut slots = String::new();
    for i in 1..=20 {
        let fv_i = float_values
            .get(i - 1)
            .map(|value| fmt_float(*value))
            .unwrap_or_else(|| "0.000000".into());
        slots += &format!(
            "                    <hkparam name=\"floatVariable{i}\">0.000000</hkparam>\n\
             \t\t\t\t    <hkparam name=\"floatValue{i}\">{fv_i}</hkparam>\n"
        );
    }
    for i in 1..=4 {
        slots += &format!(
            "                    <hkparam name=\"intVariable{i}\">0</hkparam>\n\
             \t\t\t\t    <hkparam name=\"intValue{i}\">0</hkparam>\n"
        );
    }
    format!(
        r#"
                <hkobject name="{id}" class="BSAssignVariablesModifier" signature="0x64a6ca08">
                    <hkparam name="variableBindingSet">{binding_set_ref}</hkparam>
                    <hkparam name="userData">0</hkparam>
                    <hkparam name="name">{name}</hkparam>
                    <hkparam name="enable">true</hkparam>
{slots}                </hkobject>"#
    )
}

fn build_blending_transition_effect(id: &str, name: &str) -> String {
    format!(
        r#"
                <hkobject name="{id}" class="hkbBlendingTransitionEffect" signature="0x14e54c5c">
                    <hkparam name="variableBindingSet">null</hkparam>
                    <hkparam name="userData">0</hkparam>
                    <hkparam name="name">{name}</hkparam>
                    <hkparam name="selfTransitionMode">SELF_TRANSITION_MODE_CONTINUE_IF_CYCLIC_BLEND_IF_ACYCLIC</hkparam>
                    <hkparam name="eventMode">EVENT_MODE_DEFAULT</hkparam>
                    <hkparam name="duration">0.000000</hkparam>
                    <hkparam name="toGeneratorStartTimeFraction">0.000000</hkparam>
                    <hkparam name="flags">FLAG_NONE</hkparam>
                    <hkparam name="endMode">END_MODE_NONE</hkparam>
                    <hkparam name="blendCurve">0</hkparam>
                    <hkparam name="alignmentBone">-1</hkparam>
                </hkobject>"#
    )
}

fn build_transition_info_array(
    id: &str,
    transitions: &[(usize, usize)], // (event_id, to_state_id)
    transition_eff_ref: &str,
) -> String {
    let count = transitions.len();
    let mut rows = String::new();
    for (event_id, to_state_id) in transitions {
        rows += &format!(
            r#"
                    <hkobject class="hkbStateMachineTransitionInfo" signature="0xcdec8025">
                        <hkparam name="triggerInterval">
                            <hkobject class="hkbStateMachineTimeInterval" signature="0x60a881e5">
                                <hkparam name="enterEventId">-1</hkparam>
                                <hkparam name="exitEventId">-1</hkparam>
                                <hkparam name="enterTime">0.000000</hkparam>
                                <hkparam name="exitTime">0.000000</hkparam>
                            </hkobject>
                        </hkparam>
                        <hkparam name="initiateInterval">
                            <hkobject class="hkbStateMachineTimeInterval" signature="0x60a881e5">
                                <hkparam name="enterEventId">-1</hkparam>
                                <hkparam name="exitEventId">-1</hkparam>
                                <hkparam name="enterTime">0.000000</hkparam>
                                <hkparam name="exitTime">0.000000</hkparam>
                            </hkobject>
                        </hkparam>
                        <hkparam name="transition">{transition_eff_ref}</hkparam>
                        <hkparam name="condition">null</hkparam>
                        <hkparam name="eventId">{event_id}</hkparam>
                        <hkparam name="toStateId">{to_state_id}</hkparam>
                        <hkparam name="fromNestedStateId">0</hkparam>
                        <hkparam name="toNestedStateId">0</hkparam>
                        <hkparam name="priority">0</hkparam>
                        <hkparam name="flags">0</hkparam>
                    </hkobject>"#
        );
    }
    format!(
        r#"
                <hkobject name="{id}" class="hkbStateMachineTransitionInfoArray" signature="0x704a19af">
                    <hkparam name="transitions" numelements="{count}">{rows}
                    </hkparam>
                </hkobject>"#
    )
}

fn build_state_machine_state_info(
    id: &str,
    name: &str,
    state_id: usize,
    generator_ref: &str,
    transitions_ref: &str,
) -> String {
    format!(
        r#"
                <hkobject name="{id}" class="hkbStateMachineStateInfo" signature="0x39d76713">
                    <hkparam name="variableBindingSet">null</hkparam>
                    <hkparam name="listeners" numelements="0"></hkparam>
                    <hkparam name="enterNotifyEvents">null</hkparam>
                    <hkparam name="exitNotifyEvents">null</hkparam>
                    <hkparam name="transitions">{transitions_ref}</hkparam>
                    <hkparam name="generator">{generator_ref}</hkparam>
                    <hkparam name="name">{name}</hkparam>
                    <hkparam name="stateId">{state_id}</hkparam>
                    <hkparam name="probability">1.000000</hkparam>
                    <hkparam name="enable">true</hkparam>
                </hkobject>"#
    )
}

fn build_inner_state_machine(
    id: &str,
    name: &str,
    start_state_id: usize,
    state_refs: &[String],
) -> String {
    let count = state_refs.len();
    let state_list = state_refs.join(" ");
    format!(
        r#"
                <hkobject name="{id}" class="hkbStateMachine" signature="0xa5896bcf">
                    <hkparam name="variableBindingSet">null</hkparam>
                    <hkparam name="userData">0</hkparam>
                    <hkparam name="name">{name}</hkparam>
                    <hkparam name="eventToSendWhenStateOrTransitionChanges">
                        <hkobject class="hkbEvent" signature="0x3e0fd810">
                            <hkparam name="id">-1</hkparam>
                            <hkparam name="payload">null</hkparam>
                            <hkparam name="sender">null</hkparam>
                        </hkobject>
                    </hkparam>
                    <hkparam name="startStateIdSelector">null</hkparam>
                    <hkparam name="startStateId">{start_state_id}</hkparam>
                    <hkparam name="returnToPreviousStateEventId">-1</hkparam>
                    <hkparam name="randomTransitionEventId">-1</hkparam>
                    <hkparam name="transitionToNextHigherStateEventId">-1</hkparam>
                    <hkparam name="transitionToNextLowerStateEventId">-1</hkparam>
                    <hkparam name="syncVariableIndex">-1</hkparam>
                    <hkparam name="wrapAroundStateId">false</hkparam>
                    <hkparam name="maxSimultaneousTransitions">32</hkparam>
                    <hkparam name="startStateMode">START_STATE_MODE_DEFAULT</hkparam>
                    <hkparam name="selfTransitionMode">SELF_TRANSITION_MODE_NO_TRANSITION</hkparam>
                    <hkparam name="states" numelements="{count}">{state_list}</hkparam>
                    <hkparam name="wildcardTransitions">null</hkparam>
                </hkobject>"#
    )
}

// ---------------------------------------------------------------------------
// YAML → JSON shim
// ---------------------------------------------------------------------------

/// Convert simple YAML (no anchors/aliases needed) to serde_json::Value.
/// We use `serde-saphyr` which is in the workspace.
fn serde_yaml_to_json(yaml_text: &str) -> serde_json::Value {
    match serde_saphyr::from_str::<serde_json::Value>(yaml_text) {
        Ok(v) => v,
        Err(_) => serde_json::Value::Object(Default::default()),
    }
}

fn string_list_from_value(v: Option<&serde_json::Value>) -> Vec<String> {
    v.and_then(|val| val.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn behavior_name_from_path_unique_behaviors() {
        assert_eq!(
            behavior_name_from_rel_path(
                "meshes/actors/human/UniqueBehaviors/GaussPistol/Behaviors/GaussPistol.hkx"
            ),
            "GaussPistol"
        );
    }

    #[test]
    fn behavior_name_from_path_standard() {
        assert_eq!(
            behavior_name_from_rel_path("meshes/actors/snallygaster/behaviors/snallygaster.hkx"),
            "snallygaster"
        );
    }

    #[test]
    fn drivers_yaml_loads() {
        let cfg = DriverConfig::load(DRIVERS_YAML);
        assert!(
            !cfg.telemetry_sinks.is_empty(),
            "telemetry_sinks must be non-empty"
        );
        assert!(
            cfg.variable_patterns.keys().any(|k| k.contains("overheat")),
            "fOverheatAmount pattern must be present; keys={:?}",
            cfg.variable_patterns.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn meltdown_driver_matches_minigun_heat_cycle() {
        let cfg = DriverConfig::load(DRIVERS_YAML);
        let pattern = cfg.variable_patterns.get("foverheatamount").unwrap();

        assert!(
            pattern
                .ramp_events
                .iter()
                .any(|event| event == "attackStartAuto")
        );
        assert!(
            pattern
                .ramp_events
                .iter()
                .any(|event| event == "WeaponFire")
        );
        assert!(
            pattern
                .decay_events
                .iter()
                .any(|event| event == "attackStateExit")
        );
        assert!(
            pattern
                .decay_events
                .iter()
                .any(|event| event == "triggerEnd")
        );
        assert_eq!(pattern.ramp_damping_kp, 0.005);
        assert_eq!(pattern.decay_damping_kp, 0.0025);
    }

    #[test]
    fn assign_variables_modifier_emits_heat_target_and_rate() {
        let xml = build_assign_variables_modifier("#0100", "HeatState", "#0099", &[1.0, 0.005]);

        assert!(xml.contains(r#"<hkparam name="floatValue1">1.000000</hkparam>"#));
        assert!(xml.contains(r#"<hkparam name="floatValue2">0.005000</hkparam>"#));
    }

    #[test]
    fn fmt_float_precision() {
        assert_eq!(fmt_float(0.15), "0.150000");
        assert_eq!(fmt_float(1.0), "1.000000");
        assert_eq!(fmt_float(0.0), "0.000000");
    }

    #[test]
    fn inner_state_machine_emits_state_refs_as_text_array() {
        let xml = build_inner_state_machine(
            "#0100",
            "SyncedAnimProgress_DriverStateMachine",
            0,
            &["#0098".to_string(), "#0099".to_string()],
        );

        assert!(xml.contains(r#"<hkparam name="states" numelements="2">#0098 #0099</hkparam>"#));
        assert!(!xml.contains("<hkobject>#0098</hkobject>"));
        assert!(!xml.contains("<hkobject>#0099</hkobject>"));
    }

    #[test]
    fn empty_hkx_dir_returns_empty() {
        use crate::phase::{PhaseCtx, PhaseReport};
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        let id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();

        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({});
            let source_dir = std::path::PathBuf::from("/nonexistent");
            // mod_path points to a dir with no data/meshes
            let mod_dir = std::path::PathBuf::from("/nonexistent");
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_dir,
                source_extracted_dir: &source_dir,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            SynthesizeDriversPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert_eq!(report.assets_written, 0);
        assert_eq!(report.warnings, 0);
        drop_run(id).unwrap();
    }
}
