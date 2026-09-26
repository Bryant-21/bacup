//! Fixup: play Pip-Boy voice holotapes through `Radio` scene actions, as FO4 does.
//!
//! FO76 authors a voice holotape's scene (`NOTE.DNAM` = Voice, `NOTE.SNAM` → SCEN)
//! as `Dialogue` actions on alias `-2` (no actor). FO4 has no speaker for those:
//! some tapes never start, and the rest fall back to a player-dialogue context that
//! grabs the mouse. Every `Fallout4.esm` voice holotape instead uses `Radio`
//! actions (510 of 510 Pip-Boy tape actions), laid out as
//! `ANAM=6 NAM0 ALID=-2 INAM SNAM ENAM DATA(topic) HTID(sound) DMAX(SOPM)`, with
//! `DMAX` usually `SOMDialogue2D_Holotape`.
//!
//! Only `Dialogue` actions on alias `-2` in NOTE-referenced scenes are converted;
//! the dialogue-only `DMAX`/`DMIN`/`ONAM` payloads are replaced by the radio
//! `HTID`/`DMAX` pair. A converted action's `ONAM` (FO76's holotape output model)
//! becomes its `DMAX`.

use bytes::Bytes;
use esp_authoring_core::plugin_runtime::{ParsedSubrecord, WriteEffect};
use rustc_hash::FxHashSet;
use smallvec::SmallVec;
use smol_str::SmolStr;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SigCode;
use crate::session::PluginSession;

const NOTE_TYPE_VOICE: u8 = 1;
const ACTION_DIALOGUE: u16 = 0;
const ACTION_RADIO: u16 = 6;
const NO_ACTOR_ALIAS: i32 = -2;
const SOM_DIALOGUE_2D_HOLOTAPE: u32 = 0x07_3265;
const FALLOUT4_ESM: &str = "Fallout4.esm";
/// Subrecords a radio action shares with a dialogue action; the rest are dialogue-only.
const RADIO_KEPT: &[&str] = &["NAM0", "ALID", "INAM", "FNAM", "SNAM", "ENAM", "DATA"];

pub struct ConvertHolotapeScenesToRadioFixup;

impl Fixup for ConvertHolotapeScenesToRadioFixup {
    fn name(&self) -> &'static str {
        "convert_holotape_scenes_to_radio"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        session.target_slot().parsed.game.as_deref() == Some("fo4")
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let own_master_index = session.target_masters().len() as u32;
        let Some(fallout4_index) = session
            .target_masters()
            .iter()
            .position(|master| master.eq_ignore_ascii_case(FALLOUT4_ESM))
        else {
            return Ok(report);
        };
        let default_output_model = ((fallout4_index as u32) << 24) | SOM_DIALOGUE_2D_HOLOTAPE;

        let note_sig =
            SigCode::from_str("NOTE").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let notes = session
            .form_keys_of_sig(note_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let mut scenes: Vec<u32> = Vec::new();
        let mut seen = FxHashSet::default();
        for note in notes {
            let Ok(raw) = session.raw_form_id_for_form_key(&note) else {
                continue;
            };
            let Ok(record) = session.record(raw) else {
                continue;
            };
            if let Some(scene) = voice_holotape_scene(&record.subrecords)
                && scene >> 24 == own_master_index
                && seen.insert(scene)
            {
                scenes.push(scene);
            }
        }

        let mut changed: SmallVec<[u32; 4]> = SmallVec::new();
        let mut actions = 0u32;
        for scene in scenes {
            let Ok(record) = session.record_mut(scene) else {
                continue;
            };
            if record.signature.as_str() != "SCEN" {
                continue;
            }
            let converted = convert_scene_actions(&mut record.subrecords, default_output_model);
            if converted > 0 {
                record.raw_payload = None;
                actions += converted;
                changed.push(scene);
            }
        }

        if !changed.is_empty() {
            report.records_changed = changed.len() as u32;
            session.target_slot_mut().clear_record_count_cache();
            session.record_effect(WriteEffect::RecordContents { form_ids: changed });
            report.diagnostics.push(mapper.interner.intern(&format!(
                "convert_holotape_scenes_to_radio: {actions} actions in {} holotape scenes",
                report.records_changed
            )));
        }
        Ok(report)
    }
}

/// The scene a voice holotape plays, as a raw FormID.
fn voice_holotape_scene(subrecords: &[ParsedSubrecord]) -> Option<u32> {
    let find = |sig: &str| subrecords.iter().find(|s| s.signature.as_str() == sig);
    if find("DNAM")?.data.first() != Some(&NOTE_TYPE_VOICE) {
        return None;
    }
    let scene = read_u32(&find("SNAM")?.data)?;
    (scene != 0).then_some(scene)
}

/// Rewrite every no-actor dialogue action in `subrecords` to a radio action.
/// An action opens at a typed `ANAM` and closes at the next empty `ANAM`.
fn convert_scene_actions(subrecords: &mut Vec<ParsedSubrecord>, default_output_model: u32) -> u32 {
    let original = std::mem::take(subrecords);
    let mut out = Vec::with_capacity(original.len());
    let mut converted = 0u32;
    let mut index = 0;
    while index < original.len() {
        let opens_action =
            original[index].signature.as_str() == "ANAM" && original[index].data.len() == 2;
        if !opens_action {
            out.push(original[index].clone());
            index += 1;
            continue;
        }
        let end = original[index + 1..]
            .iter()
            .position(|s| s.signature.as_str() == "ANAM")
            .map_or(original.len(), |offset| index + 1 + offset);
        let action = &original[index..end];
        match radio_action(action, default_output_model) {
            Some(radio) => {
                out.extend(radio);
                converted += 1;
            }
            None => out.extend(action.iter().cloned()),
        }
        index = end;
    }
    *subrecords = out;
    converted
}

fn radio_action(
    action: &[ParsedSubrecord],
    default_output_model: u32,
) -> Option<Vec<ParsedSubrecord>> {
    let find = |sig: &str| action.iter().find(|s| s.signature.as_str() == sig);
    if read_u16(&action[0].data)? != ACTION_DIALOGUE
        || read_u32(&find("ALID")?.data)? as i32 != NO_ACTOR_ALIAS
        || find("DATA").is_none()
    {
        return None;
    }
    let output_model = find("ONAM")
        .and_then(|onam| read_u32(&onam.data))
        .filter(|&form_id| form_id != 0)
        .unwrap_or(default_output_model);

    let mut radio = vec![subrecord("ANAM", &ACTION_RADIO.to_le_bytes())];
    radio.extend(
        action[1..]
            .iter()
            .filter(|s| RADIO_KEPT.contains(&s.signature.as_str()))
            .cloned(),
    );
    radio.push(subrecord("HTID", &0u32.to_le_bytes()));
    radio.push(subrecord("DMAX", &output_model.to_le_bytes()));
    Some(radio)
}

fn subrecord(signature: &str, data: &[u8]) -> ParsedSubrecord {
    ParsedSubrecord {
        signature: SmolStr::new(signature),
        data: Bytes::copy_from_slice(data),
        semantic_type: None,
    }
}

fn read_u16(data: &[u8]) -> Option<u16> {
    Some(u16::from_le_bytes(data.get(..2)?.try_into().ok()?))
}

fn read_u32(data: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(..4)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWN_SCENE: u32 = 0x084E_4A06;
    const FO4_HOLOTAPE_SOPM: u32 = 0x0007_3265;

    fn sub(signature: &str, data: &[u8]) -> ParsedSubrecord {
        subrecord(signature, data)
    }

    fn sigs(subrecords: &[ParsedSubrecord]) -> Vec<&str> {
        subrecords.iter().map(|s| s.signature.as_str()).collect()
    }

    fn dialogue_action(alias: i32, extra: &[ParsedSubrecord]) -> Vec<ParsedSubrecord> {
        let mut action = vec![
            sub("ANAM", &ACTION_DIALOGUE.to_le_bytes()),
            sub("NAM0", b"\0"),
            sub("ALID", &alias.to_le_bytes()),
            sub("INAM", &1u32.to_le_bytes()),
            sub("SNAM", &1u32.to_le_bytes()),
            sub("ENAM", &1u32.to_le_bytes()),
            sub("DATA", &0x084E_49E4u32.to_le_bytes()),
            sub("DMAX", &10f32.to_le_bytes()),
            sub("DMIN", &1f32.to_le_bytes()),
        ];
        action.extend(extra.iter().cloned());
        action.push(sub("ANAM", &[]));
        action
    }

    /// Mirrors MQ_Overseer_01_Vault76Scene against vanilla
    /// DN109_GunnerHolotapeBakerSc.
    #[test]
    fn no_actor_dialogue_becomes_vanilla_radio_layout() {
        let mut scene = vec![sub("FNAM", &0u32.to_le_bytes())];
        scene.extend(dialogue_action(NO_ACTOR_ALIAS, &[]));
        scene.push(sub("PNAM", &0x084E_49D8u32.to_le_bytes()));

        assert_eq!(convert_scene_actions(&mut scene, FO4_HOLOTAPE_SOPM), 1);
        assert_eq!(
            sigs(&scene),
            vec![
                "FNAM", "ANAM", "NAM0", "ALID", "INAM", "SNAM", "ENAM", "DATA", "HTID", "DMAX",
                "ANAM", "PNAM"
            ]
        );
        assert_eq!(read_u16(&scene[1].data), Some(ACTION_RADIO));
        assert_eq!(read_u32(&scene[8].data), Some(0));
        assert_eq!(read_u32(&scene[9].data), Some(FO4_HOLOTAPE_SOPM));
        assert!(scene[10].data.is_empty());

        assert_eq!(
            convert_scene_actions(&mut scene, FO4_HOLOTAPE_SOPM),
            0,
            "idempotent"
        );
    }

    #[test]
    fn onam_output_model_carries_into_dmax() {
        let mut scene = dialogue_action(
            NO_ACTOR_ALIAS,
            &[sub("ONAM", &0x0012_3456u32.to_le_bytes())],
        );
        convert_scene_actions(&mut scene, FO4_HOLOTAPE_SOPM);
        let dmax = scene
            .iter()
            .find(|s| s.signature.as_str() == "DMAX")
            .unwrap();
        assert_eq!(read_u32(&dmax.data), Some(0x0012_3456));
        assert!(!sigs(&scene).contains(&"ONAM"));
    }

    #[test]
    fn actor_dialogue_and_timers_are_untouched() {
        let mut scene = dialogue_action(0, &[]);
        scene.extend([
            sub("ANAM", &2u16.to_le_bytes()),
            sub("ALID", &NO_ACTOR_ALIAS.to_le_bytes()),
            sub("SNAM", &2f32.to_le_bytes()),
            sub("ANAM", &[]),
        ]);
        let before = scene.clone();
        assert_eq!(convert_scene_actions(&mut scene, FO4_HOLOTAPE_SOPM), 0);
        assert_eq!(sigs(&scene), sigs(&before));
    }

    #[test]
    fn only_voice_notes_name_a_scene() {
        let voice = [
            sub("DNAM", &[NOTE_TYPE_VOICE]),
            sub("SNAM", &OWN_SCENE.to_le_bytes()),
        ];
        assert_eq!(voice_holotape_scene(&voice), Some(OWN_SCENE));
        let terminal = [sub("DNAM", &[3]), sub("SNAM", &OWN_SCENE.to_le_bytes())];
        assert_eq!(voice_holotape_scene(&terminal), None);
    }
}
