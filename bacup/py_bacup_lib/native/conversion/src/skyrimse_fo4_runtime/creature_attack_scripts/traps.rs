use papyrus_core::ast::{Expr, Parameter, Pos, ScriptNode, Stmt};

pub(crate) fn required_target_scripts(
    record: &crate::record::Record,
) -> Result<std::collections::BTreeSet<String>, String> {
    let mut required = std::collections::BTreeSet::new();
    if record.sig.0 != *b"ACTI" {
        return Ok(required);
    }
    for field in record.fields.iter().filter(|field| field.sig.0 == *b"VMAD") {
        let crate::record::FieldValue::Bytes(bytes) = &field.value else {
            return Err("converted trap VMAD is not binary".into());
        };
        let mut source_binding = None;
        crate::fixups::quest_script_vmad::map_top_level_script_names(
            &mut bytes.to_vec(),
            &mut |name| {
                for (source, target) in SCRIPT_PORTS {
                    if name.eq_ignore_ascii_case(target) {
                        required.insert(source.to_ascii_lowercase());
                    } else if name.eq_ignore_ascii_case(source) {
                        source_binding = Some(name.to_string());
                    }
                }
                None
            },
        )
        .ok_or("cannot read converted trap VMAD bindings")?;
        if let Some(name) = source_binding {
            return Err(format!(
                "converted trap retains source script {name}; rerun record translation"
            ));
        }
    }
    Ok(required)
}

pub(crate) fn validate_properties(
    source: &serde_json::Value,
    target: &serde_json::Value,
    resolve: &mut impl FnMut(&str) -> Result<String, String>,
) -> Result<(), String> {
    fn map_refs(
        value: &mut serde_json::Value,
        resolve: &mut impl FnMut(&str) -> Result<String, String>,
    ) -> Result<(), String> {
        match value {
            serde_json::Value::Object(object) => {
                if let Some(reference) = object.get_mut("reference") {
                    let plugin = reference["plugin"]
                        .as_str()
                        .ok_or("invalid trap reference plugin")?;
                    let local = reference["object_id"]
                        .as_str()
                        .ok_or("invalid trap reference object")?;
                    let mapped = resolve(&format!("{local}:{plugin}"))?;
                    let (local, plugin) = mapped
                        .split_once(':')
                        .ok_or("invalid mapped trap reference")?;
                    reference["plugin"] = plugin.into();
                    reference["object_id"] = local.into();
                } else {
                    for value in object.values_mut() {
                        map_refs(value, resolve)?;
                    }
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    map_refs(value, resolve)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    let scripts = source["Scripts"]
        .as_array()
        .ok_or("missing source trap scripts")?;
    let targets = target["Scripts"]
        .as_array()
        .ok_or("missing converted trap scripts")?;
    for script in scripts {
        let name = script["ScriptName"]
            .as_str()
            .ok_or("invalid source trap script")?;
        let Some((_, target_name)) = SCRIPT_PORTS
            .iter()
            .find(|(source, _)| source.eq_ignore_ascii_case(name))
        else {
            continue;
        };
        let converted = targets
            .iter()
            .find(|script| {
                script["ScriptName"]
                    .as_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case(target_name))
            })
            .ok_or_else(|| format!("missing converted trap script {target_name}"))?;
        let mut expected = script["Properties"].clone();
        map_refs(&mut expected, resolve)?;
        if expected != converted["Properties"] {
            return Err(format!(
                "converted trap {target_name} lost source properties or dependency references"
            ));
        }
    }
    Ok(())
}

// Karthspire's puzzle override checks the player only and is not a creature perk consumer.
pub(super) const SCRIPT_PORTS: &[(&str, &str)] = &[
    ("TrapBear", "B21_SkyrimTrapBear"),
    ("TrapHitBase", "B21_SkyrimTrapHitBase"),
    ("TrapTriggerBase", "B21_SkyrimTrapTriggerBase"),
    ("PressurePlate", "B21_SkyrimPressurePlate"),
    ("TrapFirePlate", "B21_SkyrimTrapFirePlate"),
    (
        "DLC2VahlokstombTrapPlateSCRIPT",
        "B21_SkyrimVahloksTrapPlate",
    ),
];

pub(super) fn source_parent(source: &str) -> Option<&'static str> {
    match source.to_ascii_lowercase().as_str() {
        "trapbear" | "traphitbase" | "traptriggerbase" => Some("ObjectReference"),
        "pressureplate" | "trapfireplate" => Some("TrapTriggerBase"),
        "dlc2vahlokstombtrapplatescript" => Some("TrapFirePlate"),
        _ => None,
    }
}

pub(super) fn dependencies(source: &str) -> &'static [&'static str] {
    match source.to_ascii_lowercase().as_str() {
        "trapbear" => &["traphitbase"],
        "pressureplate" | "trapfireplate" => &["traptriggerbase"],
        "dlc2vahlokstombtrapplatescript" => &["trapfireplate"],
        _ => &[],
    }
}

fn register_effects() -> Stmt {
    Stmt::ExprStmt {
        expr: Expr::DotCallExpr {
            object: Box::new(Expr::NameExpr {
                name: "Self".into(),
                pos: Pos::ZERO,
            }),
            method: "RegisterForMagicEffectApplyEvent".into(),
            args: vec![Expr::NameExpr {
                name: "Self".into(),
                pos: Pos::ZERO,
            }],
            arg_names: Vec::new(),
            pos: Pos::ZERO,
        },
        pos: Pos::ZERO,
    }
}

pub(super) fn adapt(ast: &mut ScriptNode) -> Result<(), String> {
    super::ports::adapt(ast)?;
    let trigger = matches!(
        ast.name.as_str(),
        "B21_SkyrimTrapTriggerBase"
            | "B21_SkyrimPressurePlate"
            | "B21_SkyrimTrapFirePlate"
            | "B21_SkyrimVahloksTrapPlate"
    );
    for events in
        std::iter::once(&mut ast.events).chain(ast.states.iter_mut().map(|state| &mut state.events))
    {
        // Skyrim's unused OnTrapHit declaration has no FO4 event counterpart.
        events.retain(|event| {
            !(event.name.eq_ignore_ascii_case("OnTrapHit") && event.body.is_empty())
        });
        for event in events {
            if trigger && event.name.eq_ignore_ascii_case("OnCellAttach") {
                event.body.insert(0, register_effects());
            }
            if event.name.eq_ignore_ascii_case("OnMagicEffectApply") {
                if event.params.len() != 2 {
                    return Err("unexpected Skyrim trap magic event signature".into());
                }
                event.params.insert(
                    0,
                    Parameter {
                        name: "B21EffectTarget".into(),
                        ty: "ObjectReference".into(),
                        ..Default::default()
                    },
                );
                event.body.insert(0, register_effects());
            }
        }
    }
    if ast.name == "B21_SkyrimTrapTriggerBase" {
        // FO4 delivers this callback once per registration, including inactive states.
        let parsed = papyrus_core::parser::parse_script("Scriptname TrapCallbacks extends ObjectReference\nEvent OnLoad()\nRegisterForMagicEffectApplyEvent(Self)\nEndEvent\nEvent OnMagicEffectApply(ObjectReference B21EffectTarget, ObjectReference B21Caster, MagicEffect B21Effect)\nRegisterForMagicEffectApplyEvent(Self)\nEndEvent\n");
        let callbacks = parsed.ast.ok_or("cannot parse trap event adapters")?;
        ast.events.extend(callbacks.events);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
