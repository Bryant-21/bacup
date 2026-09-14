//! FNV → FO4 function/AV map loaders.
//!
//! Loads `fnv_to_fo4_script_functions.yaml` and `fnv_to_fo4_actor_values.yaml`
//! embedded in the native library. All look-ups are case-folded (lower-case
//! keys) to match the Python fallback.

use std::collections::{HashMap, HashSet};

use fnv_script_native::function_map::FunctionMap as NativeFunctionMap;

const VERIFIED_PAIR_FUNCTIONS: &str = r#"
GetDead:
  papyrus: "{self}.IsDead()"
  arg_kinds: []
  return_kind: bool
IsInCombat:
  papyrus: "{self}.IsInCombat()"
  arg_kinds: []
  return_kind: bool
IsActionRef:
  papyrus: "({arg0} == akActionRef)"
  arg_kinds: [object]
  return_kind: bool
ShowMessage:
  papyrus: "{arg0}.Show()"
  arg_kinds: [message]
  return_kind: int
GetSelf:
  papyrus: "Self"
  arg_kinds: []
  return_kind: object_reference
GetSitting:
  papyrus: "{self}.GetSitState()"
  arg_kinds: []
  return_kind: int
GetInSameCell:
  papyrus: "({self}.GetParentCell() == {arg0}.GetParentCell())"
  arg_kinds: [object_reference]
  return_kind: bool
SetObjectiveDisplayed:
  papyrus: "{arg0}.SetObjectiveDisplayed({arg1}, {arg2} != 0)"
  arg_kinds: [quest, int, int]
  return_kind: void
SetObjectiveCompleted:
  papyrus: "{arg0}.SetObjectiveCompleted({arg1}, {arg2} != 0)"
  arg_kinds: [quest, int, int]
  return_kind: void
SetRestrained:
  papyrus: "{self}.SetRestrained({arg0} != 0)"
  arg_kinds: [int]
  return_kind: void
AddToFaction:
  papyrus: "{self}.AddToFaction({arg0})"
  arg_kinds: [faction]
  return_kind: void
AddScriptPackage:
  rewrite: drop_with_warning
  reason: "FNV AddScriptPackage requires a verified converted PACK procedure and transition adapter"
  strict_failure: true
AddReputation:
  rewrite: drop_with_warning
  reason: "FO4 has no FNV reputation system"
  strict_failure: true
RewardXP:
  rewrite: drop_with_warning
  reason: "FNV RewardXP is enabled only for audited quest fragments"
  strict_failure: true
AddPerk:
  rewrite: drop_with_warning
  reason: "FNV AddPerk is enabled only for audited player-target quest fragments"
  strict_failure: true
SetPCCanUsePowerArmor:
  rewrite: drop_with_warning
  reason: "FO4 has no power-armor training gate; audited fragments use compatibility state"
  strict_failure: true
GetButtonPressed:
  rewrite: drop_with_warning
  reason: "FNV companion dialogue button polling requires an explicit FO4 UI/dialogue adapter"
  strict_failure: true
SayTo:
  rewrite: drop_with_warning
  reason: "FNV SayTo requires a verified FO4 dialogue scene adapter"
  strict_failure: true
SendAssaultAlarm:
  rewrite: drop_with_warning
  reason: "FNV SendAssaultAlarm faction semantics are not verified for FO4"
  strict_failure: true
"#;

const RENOLDS_TRIGGER_ADAPTERS: &str = r#"
AddScriptPackage:
  expansion: |
    If {arg0} == TechaticupNCRRenoldsDialoguePackage && TechaticupNCRRenoldsDialoguePackageData != None
      TechaticupNCRRenoldsDialoguePackageData.ApplyToRef({self})
      {self}.EvaluatePackage(true)
    EndIf
  arg_kinds: [package]
  return_kind: void
"#;

const HOSTAGE_ACTOR_ADAPTERS: &str = r#"
Activate:
  papyrus: "Self.Activate(akActionRef, true)"
  arg_kinds: []
  return_kind: void
ShowMessage:
  expansion: "Button = {arg0}.Show()"
  arg_kinds: [message]
  return_kind: void
GetButtonPressed:
  papyrus: "Button"
  arg_kinds: []
  return_kind: int
SayTecMineHostageFreedGreeting:
  papyrus: "{self}.SayCustom(TecMineHostageFreedGreeting, None, false, {arg0})"
  arg_kinds: [actor]
  return_kind: void
IgnoreCrime:
  expansion: |
    If {arg0} != 0
      {self}.SetCrimeFaction(None)
    Else
      {self}.SetCrimeFaction(NCRFactionNV)
    EndIf
  arg_kinds: [int]
  return_kind: void
AddToFaction:
  expansion: |
    If {arg1} == 0
      {self}.AddToFaction({arg0})
    EndIf
  arg_kinds: [faction, int]
  return_kind: void
SendAssaultAlarm:
  expansion: |
    If {arg0} == Game.GetPlayer() && {arg1} == CaesarsLegionTechMineFaction
      {arg1}.SendAssaultAlarm()
    EndIf
  arg_kinds: [actor, faction]
  return_kind: void
AddScriptPackage:
  expansion: |
    If {arg0} == TecMineHostageEscape && TecMineHostageEscapeData != None
      TecMineHostageEscapeData.ApplyToRef({self})
      {self}.EvaluatePackage(true)
    EndIf
  arg_kinds: [package]
  return_kind: void
ModRepNVNCR:
  papyrus: "FNVSliceCompat.ModRepNVNCR({arg0}, {arg1})"
  arg_kinds: [int, int]
  return_kind: void
MarkHostageDead:
  papyrus: "{self}.MarkHostageDead()"
  arg_kinds: []
  return_kind: void
AddFreedHostage:
  papyrus: "{self}.AddFreedHostage()"
  arg_kinds: []
  return_kind: void
"#;

const VTECHATTICUP_FRAGMENT_ADAPTERS: &str = r#"
RewardXP:
  papyrus: "Game.RewardPlayerXP({arg0}, false)"
  arg_kinds: [int]
  return_kind: void
CompleteQuest:
  papyrus: "{arg0}.CompleteQuest()"
  arg_kinds: [quest]
  return_kind: void
ModRepNVNCR:
  papyrus: "FNVSliceCompat.ModRepNVNCR({arg0}, {arg1})"
  arg_kinds: [int, int]
  return_kind: void
"#;

const POWER_ARMOR_FRAGMENT_ADAPTERS: &str = r#"
AddPerk:
  papyrus: "Game.GetPlayer().AddPerk({arg0}, true)"
  arg_kinds: [perk]
  return_kind: void
SetPCCanUsePowerArmor:
  papyrus: "FNVSliceCompat.SetPCCanUsePowerArmor({arg0} != 0)"
  arg_kinds: [int]
  return_kind: void
"#;

const VTECHATTICUP_INFO_ADAPTERS: &str = r#"
SetVTechatticupHostageStorVar:
  papyrus: "VTechatticup.HostageStorVar = {arg0}"
  arg_kinds: [int]
  return_kind: void
"#;

const RENOLDS_TRIGGER_SCPT_LOCAL: u32 = 0x134491;
const HOSTAGE_ACTOR_SCPT_LOCAL: u32 = 0x123191;
const VTECHATTICUP_QUST_LOCAL: u32 = 0x11F935;
const POWER_ARMOR_QUST_LOCAL: u32 = 0x06136D;
const RENOLDS_ACCEPT_INFO_LOCAL: u32 = 0x130161;
const RENOLDS_COMPLETE_INFO_LOCAL: u32 = 0x134B9B;

// ---------------------------------------------------------------------------
// FunctionEntry
// ---------------------------------------------------------------------------

/// One entry from `fnv_to_fo4_script_functions.yaml`.
///
/// Only the fields the semantic pass actually needs are captured.
#[derive(Debug, Clone)]
pub struct FunctionEntry {
    /// Papyrus call template (e.g. `"{self}.GetValue({arg0})"`).
    pub papyrus: Option<String>,
    /// Multi-line expansion template (alternative to `papyrus`).
    pub expansion: Option<String>,
    /// `"drop_with_warning"` — function has no FO4 equivalent.
    pub rewrite: Option<String>,
    /// Per-argument semantic kinds (`"actor_value"`, `"quest"`, …).
    pub arg_kinds: Vec<String>,
}

// ---------------------------------------------------------------------------
// FnvScriptContext
// ---------------------------------------------------------------------------

/// Translation context built from the two YAML map files.
///
/// Keys are case-folded (ASCII lower-case).
#[derive(Debug)]
pub struct FnvScriptContext {
    /// FNV function name (lower) → Papyrus mapping.
    pub function_map: HashMap<String, FunctionEntry>,
    /// FNV actor-value name (lower) → FO4 actor-value name.
    pub actor_value_map: HashMap<String, String>,
}

impl FnvScriptContext {
    /// Load both embedded YAML files.
    pub fn load() -> Result<Self, LoadError> {
        let function_map = load_function_map_with_adapters(None)?;
        let actor_value_map = load_actor_value_map()?;

        Ok(Self {
            function_map,
            actor_value_map,
        })
    }

    pub fn load_for_exact_slice_record(
        editor_id: &str,
        source_form_key: &str,
        mod_prefix: &str,
    ) -> Result<Self, LoadError> {
        let adapters = exact_slice_adapters(editor_id, source_form_key, mod_prefix);
        let function_map = load_function_map_with_adapters(adapters.as_deref())?;
        let actor_value_map = load_actor_value_map()?;
        Ok(Self {
            function_map,
            actor_value_map,
        })
    }
}

/// Load the shared native parser/lowerer function map plus pair-scoped
/// mappings whose FO4 semantics are explicit. Unsafe gameplay adapters remain
/// terminal `Drop` entries instead of being emitted as plausible-looking
/// Papyrus.
pub fn load_native_function_map() -> Result<NativeFunctionMap, LoadError> {
    load_native_function_map_with_adapters(None)
}

pub fn load_native_function_map_for_exact_slice_record(
    editor_id: &str,
    source_form_key: &str,
    mod_prefix: &str,
) -> Result<NativeFunctionMap, LoadError> {
    let adapters = exact_slice_adapters(editor_id, source_form_key, mod_prefix);
    load_native_function_map_with_adapters(adapters.as_deref())
}

fn load_native_function_map_with_adapters(
    adapters: Option<&str>,
) -> Result<NativeFunctionMap, LoadError> {
    let yaml = complete_function_map_yaml(adapters);
    NativeFunctionMap::from_yaml(&yaml)
        .map_err(|error| LoadError::Malformed("native function map".into(), error.to_string()))
}

pub fn load_native_actor_value_map() -> Result<HashMap<String, String>, LoadError> {
    load_actor_value_map()
}

// ---------------------------------------------------------------------------
// Loaders
// ---------------------------------------------------------------------------

fn load_function_map_with_adapters(
    adapters: Option<&str>,
) -> Result<HashMap<String, FunctionEntry>, LoadError> {
    let path = "fnv_to_fo4_script_functions.yaml";
    let text = complete_function_map_yaml(adapters);

    let raw: serde_json::Value = serde_saphyr::from_str(&text)
        .map_err(|e| LoadError::Malformed(path.to_string(), e.to_string()))?;

    let map = raw.as_object().ok_or_else(|| {
        LoadError::Malformed(
            path.to_string(),
            "expected a mapping at the top level".into(),
        )
    })?;

    let mut out = HashMap::with_capacity(map.len());
    for (name, value) in map {
        let entry = parse_function_entry(name, value)?;
        out.insert(name.to_lowercase(), entry);
    }
    Ok(out)
}

fn complete_function_map_yaml(adapters: Option<&str>) -> String {
    let mut base = include_str!("data/fnv_to_fo4_script_functions.yaml").to_string();
    base.push('\n');
    base.push_str(VERIFIED_PAIR_FUNCTIONS);
    let Some(adapters) = adapters else {
        return base;
    };
    let overrides = adapters
        .lines()
        .filter(|line| !line.starts_with(char::is_whitespace))
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && line.ends_with(':'))
        .map(|line| line.trim_end_matches(':').to_ascii_lowercase())
        .collect::<HashSet<_>>();
    let mut output = String::new();
    let mut skipping = false;
    for line in base.lines() {
        if !line.starts_with(char::is_whitespace) {
            let trimmed = line.trim();
            if trimmed.ends_with(':') && !trimmed.starts_with('#') {
                skipping = overrides.contains(&trimmed.trim_end_matches(':').to_ascii_lowercase());
            } else if !trimmed.is_empty() && !trimmed.starts_with('#') {
                skipping = false;
            }
        }
        if !skipping {
            output.push_str(line);
            output.push('\n');
        }
    }
    output.push_str(adapters);
    output
}

fn exact_slice_adapters(
    editor_id: &str,
    source_form_key: &str,
    _mod_prefix: &str,
) -> Option<String> {
    let (local, plugin) = form_key_identity(source_form_key)?;
    if !plugin.eq_ignore_ascii_case("FalloutNV.esm") {
        return None;
    }
    let adapter = match local {
        RENOLDS_TRIGGER_SCPT_LOCAL
            if editor_id.eq_ignore_ascii_case("NVTechatticupRenoldsDialogueScript") =>
        {
            Some(RENOLDS_TRIGGER_ADAPTERS)
        }
        HOSTAGE_ACTOR_SCPT_LOCAL if editor_id.eq_ignore_ascii_case("TecMineHostage") => {
            Some(HOSTAGE_ACTOR_ADAPTERS)
        }
        VTECHATTICUP_QUST_LOCAL if editor_id.eq_ignore_ascii_case("VTechatticup") => {
            Some(VTECHATTICUP_FRAGMENT_ADAPTERS)
        }
        POWER_ARMOR_QUST_LOCAL if editor_id.eq_ignore_ascii_case("FreeformPowerArmor") => {
            Some(POWER_ARMOR_FRAGMENT_ADAPTERS)
        }
        RENOLDS_ACCEPT_INFO_LOCAL | RENOLDS_COMPLETE_INFO_LOCAL
            if editor_id.eq_ignore_ascii_case("INFO") =>
        {
            Some(VTECHATTICUP_INFO_ADAPTERS)
        }
        _ => None,
    }?;
    Some(adapter.to_string())
}

fn form_key_identity(source_form_key: &str) -> Option<(u32, &str)> {
    let (local, plugin) = source_form_key.split_once([':', '@'])?;
    let local = local.trim().trim_start_matches("0x");
    Some((u32::from_str_radix(local, 16).ok()?, plugin.trim()))
}

fn parse_function_entry(name: &str, value: &serde_json::Value) -> Result<FunctionEntry, LoadError> {
    let obj = value.as_object().ok_or_else(|| {
        LoadError::Malformed(
            name.to_string(),
            "function-map entry must be a mapping".into(),
        )
    })?;

    let papyrus = obj
        .get("papyrus")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let expansion = obj
        .get("expansion")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let rewrite = obj
        .get("rewrite")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let arg_kinds = obj
        .get("arg_kinds")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    Ok(FunctionEntry {
        papyrus,
        expansion,
        rewrite,
        arg_kinds,
    })
}

fn load_actor_value_map() -> Result<HashMap<String, String>, LoadError> {
    let path = "fnv_to_fo4_actor_values.yaml";
    let text = include_str!("data/fnv_to_fo4_actor_values.yaml");

    let raw: serde_json::Value = serde_saphyr::from_str(text)
        .map_err(|e| LoadError::Malformed(path.to_string(), e.to_string()))?;

    let map = raw.as_object().ok_or_else(|| {
        LoadError::Malformed(
            path.to_string(),
            "expected a mapping at the top level".into(),
        )
    })?;

    let mut out = HashMap::with_capacity(map.len());
    for (name, value) in map {
        // null entries (e.g. Karma: null) are FNV AVs with no FO4 equivalent —
        // skip them rather than inserting.
        if let Some(mapped) = value.as_str() {
            out.insert(name.to_lowercase(), mapped.to_string());
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// LoadError
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum LoadError {
    Malformed(String, String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Malformed(path, msg) => {
                write!(f, "malformed map file {path}: {msg}")
            }
        }
    }
}

impl std::error::Error for LoadError {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_function_map_has_get_actor_value() {
        let ctx = FnvScriptContext::load().expect("load should succeed");
        let entry = ctx.function_map.get("getactorvalue");
        assert!(entry.is_some(), "GetActorValue should be in function map");
        let entry = entry.unwrap();
        assert!(
            entry.papyrus.as_deref().unwrap_or("").contains("GetValue"),
            "papyrus template should contain GetValue"
        );
        assert_eq!(
            entry.arg_kinds.first().map(String::as_str),
            Some("actor_value")
        );
    }

    #[test]
    fn load_function_map_has_get_player() {
        let ctx = FnvScriptContext::load().expect("load should succeed");
        let entry = ctx.function_map.get("getplayer");
        assert!(entry.is_some(), "GetPlayer should be in function map");
        let papyrus = entry.unwrap().papyrus.as_deref().unwrap_or("");
        assert!(papyrus.contains("Game.GetPlayer()"));
    }

    #[test]
    fn load_function_map_drop_with_warning() {
        let ctx = FnvScriptContext::load().expect("load should succeed");
        let entry = ctx.function_map.get("rewardkarma");
        assert!(entry.is_some(), "RewardKarma should be in function map");
        assert_eq!(entry.unwrap().rewrite.as_deref(), Some("drop_with_warning"));
    }

    #[test]
    fn load_actor_value_map_strength() {
        let ctx = FnvScriptContext::load().expect("load should succeed");
        let mapped = ctx.actor_value_map.get("strength");
        assert_eq!(mapped.map(String::as_str), Some("Strength"));
    }

    #[test]
    fn load_actor_value_map_null_entries_skipped() {
        let ctx = FnvScriptContext::load().expect("load should succeed");
        // Karma is null in the YAML — must not appear in the map.
        assert!(!ctx.actor_value_map.contains_key("karma"));
    }

    #[test]
    fn function_map_lookup_is_case_insensitive() {
        let ctx = FnvScriptContext::load().expect("load should succeed");
        // All keys are stored lower-case; callers must lower before lookup.
        assert!(ctx.function_map.contains_key("getisid"));
        assert!(ctx.function_map.contains_key("activate"));
    }

    #[test]
    fn shared_native_map_contains_verified_and_terminal_pair_adapters() {
        use fnv_script_native::function_map::EntryShape;

        let map = load_native_function_map().expect("native map");
        assert!(matches!(
            map.get("GetDead").map(|entry| &entry.shape),
            Some(EntryShape::Papyrus { template }) if template == "{self}.IsDead()"
        ));
        assert!(matches!(
            map.get("AddScriptPackage").map(|entry| &entry.shape),
            Some(EntryShape::Drop {
                strict_failure: true,
                ..
            })
        ));
        assert!(matches!(
            map.get("RewardXP").map(|entry| &entry.shape),
            Some(EntryShape::Drop {
                strict_failure: true,
                ..
            })
        ));
    }

    #[test]
    fn exact_slice_maps_only_the_audited_record_identities() {
        use fnv_script_native::function_map::EntryShape;

        let trigger = load_native_function_map_for_exact_slice_record(
            "NVTechatticupRenoldsDialogueScript",
            "134491:FalloutNV.esm",
            "FNV_FO3",
        )
        .unwrap();
        assert!(matches!(
            trigger.get("AddScriptPackage").map(|entry| &entry.shape),
            Some(EntryShape::Expansion { template })
                if template.contains("TechaticupNCRRenoldsDialoguePackageData.ApplyToRef({self})")
                    && template.contains("{self}.EvaluatePackage(true)")
        ));

        let hostage = load_native_function_map_for_exact_slice_record(
            "TecMineHostage",
            "123191:FalloutNV.esm",
            "FNV_FO3",
        )
        .unwrap();
        assert!(matches!(
            hostage.get("Activate").map(|entry| &entry.shape),
            Some(EntryShape::Papyrus { template })
                if template == "Self.Activate(akActionRef, true)"
        ));
        assert!(matches!(
            hostage
                .get("saytecminehostagefreedgreeting")
                .map(|entry| &entry.shape),
            Some(EntryShape::Papyrus { template })
                if template.contains("{self}.SayCustom(TecMineHostageFreedGreeting")
        ));

        let wrong_id = load_native_function_map_for_exact_slice_record(
            "NVTechatticupRenoldsDialogueScript",
            "134492:FalloutNV.esm",
            "FNV_FO3",
        )
        .unwrap();
        assert!(matches!(
            wrong_id.get("AddScriptPackage").map(|entry| &entry.shape),
            Some(EntryShape::Drop {
                strict_failure: true,
                ..
            })
        ));

        let power_armor = FnvScriptContext::load_for_exact_slice_record(
            "FreeformPowerArmor",
            "06136D:FalloutNV.esm",
            "FNV_FO3",
        )
        .unwrap();
        assert_eq!(
            power_armor.function_map["addperk"].papyrus.as_deref(),
            Some("Game.GetPlayer().AddPerk({arg0}, true)")
        );
        assert_eq!(
            power_armor.function_map["setpccanusepowerarmor"]
                .papyrus
                .as_deref(),
            Some("FNVSliceCompat.SetPCCanUsePowerArmor({arg0} != 0)")
        );

        let vtechatticup = FnvScriptContext::load_for_exact_slice_record(
            "VTechatticup",
            "11F935:FalloutNV.esm",
            "FNV_FO3",
        )
        .unwrap();
        assert_eq!(
            vtechatticup.function_map["completequest"]
                .papyrus
                .as_deref(),
            Some("{arg0}.CompleteQuest()")
        );
        assert_eq!(
            vtechatticup.function_map["modrepnvncr"].papyrus.as_deref(),
            Some("FNVSliceCompat.ModRepNVNCR({arg0}, {arg1})")
        );
        assert!(
            !vtechatticup.function_map["modrepnvncr"]
                .papyrus
                .as_deref()
                .unwrap()
                .contains("Self as")
        );
        let vtechatticup_wrong_plugin = FnvScriptContext::load_for_exact_slice_record(
            "VTechatticup",
            "11F935:FalloutNV.esm",
            "FNV_FO3",
        )
        .unwrap();
        assert_eq!(
            vtechatticup_wrong_plugin.function_map["completequest"]
                .papyrus
                .as_deref(),
            Some("{arg0}.CompleteAllObjectives()")
        );

        let wrong_plugin = FnvScriptContext::load_for_exact_slice_record(
            "FreeformPowerArmor",
            "06136D:Other.esm",
            "FNV_FO3",
        )
        .unwrap();
        assert_eq!(
            wrong_plugin.function_map["setpccanusepowerarmor"]
                .rewrite
                .as_deref(),
            Some("drop_with_warning")
        );
    }
}
