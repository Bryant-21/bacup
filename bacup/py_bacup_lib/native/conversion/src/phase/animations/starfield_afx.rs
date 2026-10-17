use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(super) struct SourceEvent {
    pub frame: f64,
    pub name: String,
    pub payload: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(super) struct SourceAudio {
    pub frame: f64,
    pub foley: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(super) struct SourceVariable {
    pub name: String,
    pub start_frame: f64,
    pub end_frame: f64,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct AfxContract {
    pub tag: String,
    pub filename: String,
    pub is_state: bool,
    pub rampout_frames: f64,
    pub compression: Option<u32>,
    pub flags: Vec<String>,
    pub events: Vec<SourceEvent>,
    pub audios: Vec<SourceAudio>,
    pub variables: Vec<SourceVariable>,
    pub unsupported_xml: Vec<String>,
}

fn text(node: roxmltree::Node<'_, '_>, name: &str) -> Result<String, String> {
    let mut matching = node.children().filter(|child| child.has_tag_name(name));
    let value = matching
        .next()
        .ok_or_else(|| format!("AFX missing {name}"))?;
    if matching.next().is_some() {
        return Err(format!("AFX duplicate {name}"));
    }
    Ok(value.text().unwrap_or("").trim().to_owned())
}

fn number(node: roxmltree::Node<'_, '_>, name: &str) -> Result<f64, String> {
    let value: f64 = text(node, name)?
        .parse()
        .map_err(|_| format!("AFX invalid {name}"))?;
    if !value.is_finite() || value < 0. {
        return Err(format!("AFX invalid {name} {value}"));
    }
    Ok(value)
}

fn children<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
) -> impl Iterator<Item = roxmltree::Node<'a, 'input>> {
    node.children().filter(roxmltree::Node::is_element)
}

pub(super) fn parse_afx(xml: &str, last_frame: f64) -> Result<AfxContract, String> {
    if !last_frame.is_finite() || last_frame < 0. {
        return Err("AFX invalid animation end frame".into());
    }
    let doc = roxmltree::Document::parse(xml).map_err(|error| format!("AFX XML: {error}"))?;
    let root = doc.root_element();
    if !root.has_tag_name("root") {
        return Err("AFX requires root element".into());
    }
    let is_state = match text(root, "is_state")?.as_str() {
        "0" => false,
        "1" => true,
        value => return Err(format!("AFX invalid is_state {value}")),
    };
    let mut contract = AfxContract {
        tag: text(root, "tag")?,
        filename: text(root, "filename")?,
        is_state,
        rampout_frames: number(root, "rampout")?,
        compression: None,
        flags: Vec::new(),
        events: Vec::new(),
        audios: Vec::new(),
        variables: Vec::new(),
        unsupported_xml: Vec::new(),
    };
    let mut groups = std::collections::HashSet::new();
    for node in children(root) {
        let name = node.tag_name().name();
        if !groups.insert(name) {
            return Err(format!("AFX duplicate {name}"));
        }
        match name {
            "is_state" | "tag" | "filename" | "rampout" => {}
            "compression" => {
                contract.compression = Some(
                    node.text()
                        .unwrap_or("")
                        .trim()
                        .parse()
                        .map_err(|_| "AFX invalid compression")?,
                )
            }
            "flags" => {
                for flag in children(node) {
                    if flag.has_tag_name("flag") {
                        contract
                            .flags
                            .push(flag.text().unwrap_or("").trim().to_owned());
                    } else {
                        contract.unsupported_xml.push(xml[flag.range()].to_owned());
                    }
                }
            }
            "events" => {
                for event in children(node) {
                    if !event.has_tag_name("event") {
                        contract.unsupported_xml.push(xml[event.range()].to_owned());
                        continue;
                    }
                    contract.events.push(SourceEvent {
                        frame: number(event, "frame")?,
                        name: text(event, "name")?,
                        payload: text(event, "payload")?,
                    });
                    for extra in children(event).filter(|child| {
                        !matches!(child.tag_name().name(), "frame" | "name" | "payload")
                    }) {
                        contract.unsupported_xml.push(xml[extra.range()].to_owned());
                    }
                }
            }
            "audios" => {
                for audio in children(node) {
                    if !audio.has_tag_name("audio") {
                        contract.unsupported_xml.push(xml[audio.range()].to_owned());
                        continue;
                    }
                    contract.audios.push(SourceAudio {
                        frame: number(audio, "frame")?,
                        foley: text(audio, "foley")?,
                    });
                    for extra in children(audio)
                        .filter(|child| !matches!(child.tag_name().name(), "frame" | "foley"))
                    {
                        contract.unsupported_xml.push(xml[extra.range()].to_owned());
                    }
                }
            }
            "variables" => {
                for variable in children(node) {
                    if !variable.has_tag_name("variable") {
                        contract
                            .unsupported_xml
                            .push(xml[variable.range()].to_owned());
                        continue;
                    }
                    let value = SourceVariable {
                        name: text(variable, "name")?,
                        start_frame: number(variable, "start")?,
                        end_frame: number(variable, "end")?,
                    };
                    if value.start_frame > value.end_frame {
                        return Err(format!("AFX variable {} has reversed interval", value.name));
                    }
                    contract.variables.push(value);
                    for extra in children(variable).filter(|child| {
                        !matches!(child.tag_name().name(), "name" | "start" | "end")
                    }) {
                        contract.unsupported_xml.push(xml[extra.range()].to_owned());
                    }
                }
            }
            _ => contract.unsupported_xml.push(xml[node.range()].to_owned()),
        }
    }
    if contract
        .events
        .iter()
        .map(|event| event.frame)
        .chain(contract.audios.iter().map(|audio| audio.frame))
        .chain(contract.variables.iter().map(|variable| variable.end_frame))
        .any(|frame| frame > last_frame + 0.0001)
    {
        return Err("AFX annotation extends beyond AF duration".into());
    }
    if contract.tag.is_empty() || contract.filename.is_empty() {
        return Err("AFX requires nonempty tag and filename".into());
    }
    Ok(contract)
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct AnnotationPlan {
    pub annotations: Vec<(f64, String)>,
    pub unresolved: Vec<String>,
    pub functionality_certified: bool,
}

pub(super) fn annotation_plan(
    contract: &AfxContract,
    fps: f64,
    speed: f64,
) -> Result<AnnotationPlan, String> {
    if !fps.is_finite() || fps <= 0. || !speed.is_finite() || speed <= 0. {
        return Err("invalid AFX playback rate".into());
    }
    let mut plan = AnnotationPlan {
        annotations: Vec::new(),
        unresolved: Vec::new(),
        functionality_certified: false,
    };
    for event in &contract.events {
        let target = match (event.name.as_str(), event.payload.as_str()) {
            ("WeaponFire", "") => Some("weaponFire"),
            ("ReloadComplete", "") => Some("reloadComplete"),
            _ => None,
        };
        match target {
            Some(target) => plan
                .annotations
                .push((event.frame / fps / speed, target.into())),
            None => plan.unresolved.push(format!(
                "source_event:{}:{}@{}",
                event.name, event.payload, event.frame
            )),
        }
    }
    plan.unresolved.extend(
        contract
            .audios
            .iter()
            .map(|audio| format!("wwise_foley:{}@{}", audio.foley, audio.frame)),
    );
    plan.unresolved
        .extend(contract.variables.iter().map(|variable| {
            format!(
                "graph_variable:{}@{}..{}",
                variable.name, variable.start_frame, variable.end_frame
            )
        }));
    plan.unresolved.extend(
        contract
            .unsupported_xml
            .iter()
            .map(|xml| format!("afx_xml:{xml}")),
    );
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIRE: &str = "<root><is_state>0</is_state><tag>FireSingle</tag><filename>FireSingle.af</filename><rampout>10</rampout><flags><flag>Additive</flag></flags><events><event><frame>0</frame><name>WeaponFire</name><payload></payload></event><event><frame>1</frame><name>WeaponFireEnd</name><payload></payload></event></events></root>";

    #[test]
    fn source_flags_and_unmapped_events_are_retained() {
        let source = parse_afx(FIRE, 25.).unwrap();
        assert_eq!(source.flags, ["Additive"]);
        let plan = annotation_plan(&source, 30., 2.).unwrap();
        assert_eq!(plan.annotations, [(0., "weaponFire".into())]);
        assert_eq!(plan.unresolved, ["source_event:WeaponFireEnd:@1"]);
        assert!(!plan.functionality_certified);
    }

    #[test]
    fn malformed_and_out_of_range_contracts_fail() {
        for bad in [
            FIRE.replace("<frame>1</frame>", "<frame>26</frame>"),
            FIRE.replace("<frame>1</frame>", "<frame>NaN</frame>"),
            FIRE.replace(
                "<tag>FireSingle</tag>",
                "<tag>FireSingle</tag><tag>Other</tag>",
            ),
        ] {
            assert!(parse_afx(&bad, 25.).is_err());
        }
    }

    #[test]
    fn unknown_payload_and_xml_are_not_silently_lowered() {
        let source = parse_afx(
            &FIRE
                .replace("<payload></payload>", "<payload>OtherSlot</payload>")
                .replace("</root>", "<unknown>17</unknown></root>"),
            25.,
        )
        .unwrap();
        let plan = annotation_plan(&source, 30., 1.).unwrap();
        assert!(plan.annotations.is_empty());
        assert_eq!(source.unsupported_xml, ["<unknown>17</unknown>"]);
        assert_eq!(plan.unresolved.len(), 3);
    }

    #[test]
    #[ignore = "requires extracted Starfield animations"]
    fn real_stinger_reload_has_source_transfer_audio_and_visibility_contracts() {
        for perspective in ["", "_1stperson/"] {
            let path = format!(
                "{}/actors/human/{perspective}animations/gun/grippistol/stinger/reload.afx", std::env::var("B21_STARFIELD_MESHES").expect("B21_STARFIELD_MESHES")
            );
            let source = parse_afx(&std::fs::read_to_string(path).unwrap(), 101.).unwrap();
            assert!(!source.flags.iter().any(|flag| flag == "Additive"));
            let plan = annotation_plan(&source, 30., 1.).unwrap();
            assert_eq!(plan.annotations, [(50. / 30., "reloadComplete".into())]);
            assert_eq!(
                source.audios.len(),
                if perspective.is_empty() { 6 } else { 8 }
            );
            assert_eq!(source.variables[0].name, "bIsReloading");
            if perspective.is_empty() {
                assert!(source.events.iter().any(|event| event.name == "CullBone"
                    && event.payload == "Magazine"
                    && event.frame == 25.));
            }
            assert!(!plan.functionality_certified);
        }
    }
}
