use super::*;

pub(super) fn validate(manifest: &CreatureManifest, xml: &str, errors: &mut ValidationErrors) {
    let Ok(document) = roxmltree::Document::parse(xml) else {
        return;
    };
    for node in document
        .descendants()
        .filter(|node| node.attribute("class") == Some("hkbClipGenerator"))
    {
        let parameter = |name| {
            node.children()
                .find(|child| child.attribute("name") == Some(name))
                .and_then(|child| child.text())
                .unwrap_or("")
        };
        let Some(clip) = manifest
            .clips
            .iter()
            .find(|clip| internal_path(manifest, &clip.path) == parameter("animationName"))
        else {
            continue;
        };
        let bindings = manifest
            .property_sequences
            .iter()
            .filter(|binding| binding.clip_name == clip.name)
            .collect::<Vec<_>>();
        if bindings.is_empty() {
            continue;
        }
        let start = parameter("cropStartAmountLocalTime").parse::<f32>().ok();
        let end = parameter("cropEndAmountLocalTime").parse::<f32>().ok();
        let looping = parameter("mode") == "MODE_LOOPING";
        let binding = bindings.iter().find(|binding| {
            Some(binding.crop_start_bits) == start.map(f32::to_bits)
                && Some(binding.crop_end_bits) == end.map(f32::to_bits)
                && binding.looping == looping
        });
        if binding.is_none()
            || binding.is_some_and(|binding| {
                !binding.sequence_names.is_empty()
                    && !parameter("name").starts_with("B21PropertyClock_")
            })
        {
            errors.push(
                "missing_property_sequence_window",
                format!(
                    "clip {} has no NIF sequence for the emitted crop/loop window",
                    clip.name
                ),
            );
        }
    }
}

pub(super) fn wrap(
    manifest: &CreatureManifest,
    clip: &ClipDecl,
    id: &str,
    crop_start: f32,
    crop_end: f32,
    mut xml: String,
) -> String {
    let Some(binding) = manifest.property_sequences.iter().find(|binding| {
        binding.clip_name == clip.name
            && binding.looping == clip.looping
            && binding.crop_start_bits == crop_start.to_bits()
            && binding.crop_end_bits == crop_end.to_bits()
    }) else {
        return xml;
    };
    if binding.sequence_names.is_empty() {
        return xml;
    }
    let suffix = id.trim_start_matches('#');
    let clock_id = format!("#property_clock_{suffix}");
    xml = xml.replacen(
        &format!("name=\"{id}\""),
        &format!("name=\"{clock_id}\""),
        1,
    );
    let name_start = xml.find("<hkparam name=\"name\">").unwrap() + "<hkparam name=\"name\">".len();
    let name_end = name_start + xml[name_start..].find("</hkparam>").unwrap();
    xml.replace_range(name_start..name_end, &format!("B21PropertyClock_{suffix}"));
    let mut children = Vec::new();
    for index in 0..=binding.sequence_names.len() {
        let generator = if index == 0 {
            clock_id.clone()
        } else {
            let generator = format!("#property_sequence_{suffix}_{index}");
            xml.push_str(&format!("<hkobject name=\"{generator}\" class=\"BGSGamebryoSequenceGenerator\" signature=\"0x4e708fb6\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">B21PropertySequence_{suffix}</hkparam><hkparam name=\"pSequence\">{sequence}__{suffix}</hkparam><hkparam name=\"eBlendModeFunction\">BMF_PERCENT</hkparam><hkparam name=\"fPercent\">1</hkparam><hkparam name=\"eUseTimePercentage\">NOT_USING_TIME_PERCENTAGE</hkparam><hkparam name=\"fTimePercent\">0</hkparam></hkobject>\n", sequence=escape_xml(&binding.sequence_names[index-1])));
            generator
        };
        let child = format!("#property_child_{suffix}_{index}");
        xml.push_str(&format!("<hkobject name=\"{child}\" class=\"hkbBlenderGeneratorChild\" signature=\"0xb35bbfd3\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"generator\">{generator}</hkparam><hkparam name=\"boneWeights\">null</hkparam><hkparam name=\"weight\">1</hkparam><hkparam name=\"worldFromModelWeight\">{world}</hkparam></hkobject>\n", world=usize::from(index==0)));
        children.push(child);
    }
    xml.push_str(&format!("<hkobject name=\"{id}\" class=\"hkbBlenderGenerator\" signature=\"0xce45c088\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">B21PropertyPair_{suffix}</hkparam><hkparam name=\"referencePoseWeightThreshold\">0</hkparam><hkparam name=\"blendParameter\">0</hkparam><hkparam name=\"minCyclicBlendParameter\">0</hkparam><hkparam name=\"maxCyclicBlendParameter\">0</hkparam><hkparam name=\"indexOfSyncMasterChild\">0</hkparam><hkparam name=\"flags\">1</hkparam><hkparam name=\"subtractLastChild\">false</hkparam><hkparam name=\"children\" numelements=\"{count}\">{children}</hkparam></hkobject>\n", count=children.len(), children=children.join(" ")));
    xml
}
