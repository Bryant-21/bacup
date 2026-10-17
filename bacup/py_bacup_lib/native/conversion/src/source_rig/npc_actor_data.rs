use super::*;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureNpcActorData {
    pub flags: u32,
    pub disposition_base: i16,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub factions: Vec<CreatureNpcFaction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_data: Option<[u8; 24]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packages: Vec<CreatureTargetRecordReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combat_style: Option<CreatureTargetRecordReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_type: Option<CreatureTargetRecordReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<CreatureNpcScriptAttachment>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureNpcScriptAttachment {
    pub source_script: CreatureSourceRecordReference,
    pub target_vmad: Vec<u8>,
    pub dependencies: Vec<CreatureTargetRecordReference>,
    pub target_masters: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureNpcFaction {
    pub faction: CreatureTargetRecordReference,
    pub rank: i8,
}

impl CreatureNpcActorData {
    pub fn dependencies(&self) -> Vec<CreatureTargetRecordReference> {
        self.factions
            .iter()
            .map(|entry| entry.faction.clone())
            .chain(self.packages.iter().cloned())
            .chain(self.combat_style.iter().cloned())
            .chain(self.voice_type.iter().cloned())
            .chain(
                self.script
                    .iter()
                    .flat_map(|script| script.dependencies.iter().cloned()),
            )
            .collect()
    }

    pub(super) fn validate(
        &self,
        manifest: &CreatureRecordManifest,
    ) -> Result<(), CreatureRecordError> {
        for entry in &self.factions {
            validate_target_record_reference_at(
                &entry.faction,
                "FACT",
                manifest,
                "NPC_",
                "SNAM.faction",
            )?;
        }
        for package in &self.packages {
            validate_target_record_reference_at(package, "PACK", manifest, "NPC_", "PKID")?;
        }
        if let Some(style) = &self.combat_style {
            validate_target_record_reference_at(style, "CSTY", manifest, "NPC_", "ZNAM")?;
        }
        if let Some(voice) = &self.voice_type {
            validate_target_record_reference_at(voice, "VTYP", manifest, "NPC_", "VTCK")?;
        }
        if let Some(script) = &self.script {
            if script.source_script.signature != "SCPT"
                || !script.source_script.source_identity.is_valid()
            {
                return Err(invalid(
                    "npc_script_source",
                    "NPC script attachment requires an exact source SCPT identity",
                ));
            }
            let masters = script
                .target_masters
                .iter()
                .map(|master| master.to_ascii_lowercase())
                .collect::<HashSet<_>>();
            if masters.len() != script.target_masters.len()
                || masters.contains("")
                || masters.contains(&manifest.target_plugin.to_ascii_lowercase())
            {
                return Err(invalid(
                    "npc_script_masters",
                    "NPC script attachment has an invalid target master order",
                ));
            }
            if esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json(
                &script.target_vmad, &script.target_masters, &manifest.target_plugin, Some("NPC_")
            ).is_none() {
                return Err(invalid("npc_script_vmad", "NPC script attachment has invalid target VMAD bytes"));
            }
            for reference in &script.dependencies {
                if SigCode::from_str(&reference.signature).is_err()
                    || reference.form_key.local == 0
                    || reference.form_key.local > 0x00ff_ffff
                    || (!reference
                        .form_key
                        .plugin
                        .eq_ignore_ascii_case(&manifest.target_plugin)
                        && !masters.contains(&reference.form_key.plugin.to_ascii_lowercase()))
                {
                    return Err(invalid(
                        "npc_script_dependency",
                        "NPC script dependency is not a valid target record reference",
                    ));
                }
            }
        }
        Ok(())
    }

    pub(super) fn fields(&self, interner: &StringInterner) -> Vec<FieldEntry> {
        let mut fields = self
            .factions
            .iter()
            .map(|entry| {
                struct_field(
                    "SNAM",
                    vec![
                        (
                            "faction",
                            FieldValue::FormKey(entry.faction.form_key.intern(interner)),
                        ),
                        ("rank", FieldValue::Int(i64::from(entry.rank))),
                    ],
                    interner,
                )
            })
            .collect::<Vec<_>>();
        if let Some(ai_data) = self.ai_data {
            fields.push(bytes_field("AIDT", ai_data.to_vec()));
        }
        fields.extend(
            self.packages
                .iter()
                .map(|package| form_field("PKID", package.form_key.intern(interner))),
        );
        if let Some(style) = &self.combat_style {
            fields.push(form_field("ZNAM", style.form_key.intern(interner)));
        }
        if let Some(voice) = &self.voice_type {
            fields.push(form_field("VTCK", voice.form_key.intern(interner)));
        }
        if let Some(script) = &self.script {
            fields.push(bytes_field("VMAD", script.target_vmad.clone()));
        }
        fields
    }
}

pub(super) fn verify_actor_data(
    variant: &CreatureNpcRecordVariant,
    actual: Option<&Record>,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<(), CreatureRecordError> {
    if variant.actor_data.is_none() {
        return Ok(());
    }
    let key = variant.form_key.intern(interner);
    let mut expected = emit_npc_record_variant(variant, key, key, &[], &[], None, interner);
    normalize_records(std::slice::from_mut(&mut expected), schema, interner)?;
    let actual =
        actual.ok_or_else(|| invalid("npc_actor_data_closure", "projected NPC is missing"))?;
    let actor_fields = |record: &Record| {
        record
            .fields
            .iter()
            .filter(|field| {
                matches!(
                    field.sig.as_str(),
                    "SNAM" | "AIDT" | "PKID" | "ZNAM" | "VTCK" | "VMAD"
                )
            })
            .cloned()
            .collect::<Vec<_>>()
    };
    if actor_fields(actual) != actor_fields(&expected) {
        return Err(invalid(
            "npc_actor_data_closure",
            format!(
                "projected NPC actor fields differ for {}",
                variant.source_identity.stable_key()
            ),
        ));
    }
    for name in ["flags", "disposition_base"] {
        let actual_value =
            field(actual, "ACBS").and_then(|value| struct_member(value, name, interner));
        let expected_value =
            field(&expected, "ACBS").and_then(|value| struct_member(value, name, interner));
        if actual_value.is_none() || actual_value != expected_value {
            return Err(invalid(
                "npc_actor_data_closure",
                format!(
                    "projected NPC ACBS.{name} differs for {}",
                    variant.source_identity.stable_key()
                ),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "npc_actor_data_tests.rs"]
mod tests;
