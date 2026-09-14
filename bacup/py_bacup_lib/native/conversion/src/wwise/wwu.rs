//! `.wwu` (Wwise Work Unit) XML emission. Bethesda's project is plain
//! schema-versioned XML designed for source control, so these are
//! hand-templated strings matching the vanilla shapes rather than a generic
//! XML-writer dependency.

/// One audio source about to become a Sound + Event pair.
pub struct EventDraft {
    pub event_name: String,
    pub sound_guid: String,
    pub source_guid: String,
    pub event_guid: String,
    pub action_guid: String,
    pub media_id: u32,
    /// Path relative to `Originals\SFX\`, backslash-separated (e.g.
    /// `FO4SF\TRACK.wav`).
    pub audio_file_rel: String,
}

/// Vanilla bus / conversion-preset GUIDs a build must target so
/// generated objects route through the game's already-loaded Init bank.
pub const BUS_MUS_GUID: &str = "A8587953-1AEA-47DC-BB2B-6192068D0232";
pub const BUS_MUS_WU_GUID: &str = "A8CA8A86-3032-4375-983D-6AB506E30DDA";
pub const CONVERSION_MUS_SCORE_GUID: &str = "D855D15C-81D0-4D44-820E-F118E083E66A";
pub const CONVERSION_MUS_SCORE_WU_GUID: &str = "51585EB9-4FFF-40CE-9732-ECE6F23789C7";

fn braced(guid: &str) -> String {
    format!("{{{guid}}}")
}

/// Actor-Mixer Hierarchy/FO4SF.wwu — one ActorMixer wrapping one `<Sound>`
/// per draft, each with a pinned MediaID.
pub fn actor_mixer_wwu(wu_guid: &str, mixer_guid: &str, drafts: &[EventDraft]) -> String {
    let mut sounds = String::new();
    for d in drafts {
        sounds.push_str(&format!(
            r#"      <Sound Name="{name}" ID="{sound_id}">
       <PropertyList>
        <Property Name="IsStreamingEnabled" Type="bool"><ValueList><Value>True</Value></ValueList></Property>
       </PropertyList>
       <ChildrenList>
        <AudioFileSource Name="{name}" ID="{source_id}">
         <Language>SFX</Language>
         <AudioFile>{audio_file}</AudioFile>
         <MediaIDList><MediaID ID="{media_id}"/></MediaIDList>
        </AudioFileSource>
       </ChildrenList>
       <ObjectLists/>
       <ActiveSourceList><ActiveSource Name="{name}" ID="{source_id}" Platform="Linked"/></ActiveSourceList>
      </Sound>
"#,
            name = d.event_name,
            sound_id = braced(&d.sound_guid),
            source_id = braced(&d.source_guid),
            audio_file = d.audio_file_rel,
            media_id = d.media_id,
        ));
    }
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<WwiseDocument Type="WorkUnit" ID="{wu_id}" SchemaVersion="103">
 <AudioObjects>
  <WorkUnit Name="FO4SF" ID="{wu_id}" PersistMode="Standalone">
   <ChildrenList>
    <ActorMixer Name="FO4SF_mixer" ID="{mixer_id}">
     <ReferenceList>
      <Reference Name="Conversion"><ObjectRef Name="MUS_Score" ID="{conversion_id}" WorkUnitID="{conversion_wu_id}"/></Reference>
      <Reference Name="OutputBus"><ObjectRef Name="Bus_MUS" ID="{bus_id}" WorkUnitID="{bus_wu_id}"/></Reference>
     </ReferenceList>
     <ChildrenList>
{sounds}    </ChildrenList>
    </ActorMixer>
   </ChildrenList>
  </WorkUnit>
 </AudioObjects>
</WwiseDocument>
"#,
        wu_id = braced(wu_guid),
        mixer_id = braced(mixer_guid),
        conversion_id = braced(CONVERSION_MUS_SCORE_GUID),
        conversion_wu_id = braced(CONVERSION_MUS_SCORE_WU_GUID),
        bus_id = braced(BUS_MUS_GUID),
        bus_wu_id = braced(BUS_MUS_WU_GUID),
    )
}

/// Events/FO4SF.wwu — one `<Event>` per draft with a single ActionType=1
/// (Play) action targeting the matching Sound.
pub fn events_wwu(wu_guid: &str, amh_wu_guid: &str, drafts: &[EventDraft]) -> String {
    let mut events = String::new();
    for d in drafts {
        events.push_str(&format!(
            r#"    <Event Name="{name}" ID="{event_id}">
     <ChildrenList>
      <Action Name="" ID="{action_id}">
       <PropertyList><Property Name="ActionType" Type="int16" Value="1"/></PropertyList>
       <ReferenceList><Reference Name="Target">
         <ObjectRef Name="{name}" ID="{sound_id}" WorkUnitID="{amh_wu_id}"/>
       </Reference></ReferenceList>
      </Action>
     </ChildrenList>
    </Event>
"#,
            name = d.event_name,
            event_id = braced(&d.event_guid),
            action_id = braced(&d.action_guid),
            sound_id = braced(&d.sound_guid),
            amh_wu_id = braced(amh_wu_guid),
        ));
    }
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<WwiseDocument Type="WorkUnit" ID="{wu_id}" SchemaVersion="103">
 <AudioObjects>
  <WorkUnit Name="FO4SF" ID="{wu_id}" PersistMode="Standalone">
   <ChildrenList>
{events}   </ChildrenList>
  </WorkUnit>
 </AudioObjects>
</WwiseDocument>
"#,
        wu_id = braced(wu_guid),
    )
}

/// The `<SoundBank>` entry appended to `SoundBanks/Default Work Unit.wwu`.
/// `Filter="3"` = include the referenced objects + their
/// events, excluding buses/effects — matches the vanilla `Starfield_FST` /
/// `Starfield_WPN` declarations.
pub fn soundbank_entry_xml(
    bank_name: &str,
    bank_guid: &str,
    amh_wu_guid: &str,
    events_wu_guid: &str,
) -> String {
    format!(
        r#"    <SoundBank Name="{bank_name}" ID="{bank_id}">
     <ObjectInclusionList>
      <ObjectRef Name="FO4SF" ID="{amh_id}" WorkUnitID="{amh_id}" Filter="3"/>
      <ObjectRef Name="FO4SF" ID="{events_id}" WorkUnitID="{events_id}" Filter="3"/>
     </ObjectInclusionList>
     <ObjectExclusionList/><GameSyncExclusionList/>
    </SoundBank>
"#,
        bank_id = braced(bank_guid),
        amh_id = braced(amh_wu_guid),
        events_id = braced(events_wu_guid),
    )
}

/// Insert `entry_xml` into the project's SoundBanks work unit. Inserts
/// before the last `</ChildrenList>` when the file already has one (the
/// vanilla shape — SoundBank entries are direct children of the work unit's
/// top-level `<ChildrenList>`); otherwise synthesizes a minimal wrapper.
pub fn append_soundbank_entry(existing: Option<&str>, entry_xml: &str) -> String {
    if let Some(content) = existing {
        if let Some(pos) = content.rfind("</ChildrenList>") {
            let mut out = String::with_capacity(content.len() + entry_xml.len());
            out.push_str(&content[..pos]);
            out.push_str(entry_xml);
            out.push_str(&content[pos..]);
            return out;
        }
    }
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<WwiseDocument Type="WorkUnit" ID="{{00000000-0000-0000-0000-000000000000}}" SchemaVersion="103">
 <AudioObjects>
  <WorkUnit Name="Default Work Unit" ID="{{00000000-0000-0000-0000-000000000000}}" PersistMode="Standalone">
   <ChildrenList>
{entry_xml}   </ChildrenList>
  </WorkUnit>
 </AudioObjects>
</WwiseDocument>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> EventDraft {
        EventDraft {
            event_name: "FO4SF_MUS_TRACK".into(),
            sound_guid: "11111111-1111-1111-1111-111111111111".into(),
            source_guid: "22222222-2222-2222-2222-222222222222".into(),
            event_guid: "33333333-3333-3333-3333-333333333333".into(),
            action_guid: "44444444-4444-4444-4444-444444444444".into(),
            media_id: 12345,
            audio_file_rel: "FO4SF\\TRACK.wav".into(),
        }
    }

    #[test]
    fn actor_mixer_wwu_carries_pinned_media_id() {
        let xml = actor_mixer_wwu(
            "aaaaaaaa-0000-0000-0000-000000000000",
            "bbbbbbbb-0000-0000-0000-000000000000",
            &[draft()],
        );
        assert!(xml.contains(r#"MediaID ID="12345""#));
        assert!(xml.contains("FO4SF_MUS_TRACK"));
        assert!(xml.contains(CONVERSION_MUS_SCORE_GUID));
        assert!(xml.contains(BUS_MUS_GUID));
    }

    #[test]
    fn events_wwu_action_type_is_play() {
        let xml = events_wwu(
            "cccccccc-0000-0000-0000-000000000000",
            "aaaaaaaa-0000-0000-0000-000000000000",
            &[draft()],
        );
        assert!(xml.contains(r#"Property Name="ActionType" Type="int16" Value="1""#));
        assert!(xml.contains("33333333-3333-3333-3333-333333333333"));
    }

    #[test]
    fn soundbank_entry_uses_filter_three() {
        let xml = soundbank_entry_xml(
            "Fallout4_SF",
            "dddddddd-0000-0000-0000-000000000000",
            "aaaaaaaa-0000-0000-0000-000000000000",
            "cccccccc-0000-0000-0000-000000000000",
        );
        assert!(xml.contains(r#"Filter="3""#));
        assert!(xml.contains("Fallout4_SF"));
    }

    #[test]
    fn append_soundbank_entry_inserts_before_last_children_close() {
        let existing =
            "<WorkUnit><ChildrenList><SoundBank Name=\"Init\"/></ChildrenList></WorkUnit>";
        let out = append_soundbank_entry(Some(existing), "<SoundBank Name=\"New\"/>");
        assert!(out.contains("Init"));
        assert!(out.contains("New"));
        assert!(out.find("Init").unwrap() < out.find("New").unwrap());
    }

    #[test]
    fn append_soundbank_entry_synthesizes_wrapper_when_missing() {
        let out = append_soundbank_entry(None, "<SoundBank Name=\"New\"/>");
        assert!(out.contains("<WwiseDocument"));
        assert!(out.contains("New"));
    }
}
