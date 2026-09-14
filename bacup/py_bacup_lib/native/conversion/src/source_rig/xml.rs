use std::collections::{BTreeMap, BTreeSet};

use havok_native::hkx::descriptors::DescriptorRegistry;

use super::manifest::{ValidationErrors, VariableType};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HavokXmlFacts {
    pub classversion: String,
    pub contentsversion: String,
    pub object_ids: BTreeSet<String>,
    pub references: BTreeSet<String>,
    pub event_names: Vec<String>,
    pub variable_names: Vec<String>,
    pub character_property_names: Vec<String>,
}

#[derive(Clone, Debug, Default)]
struct XmlNode {
    name: String,
    attributes: BTreeMap<String, String>,
    text: String,
    children: Vec<XmlNode>,
}

pub fn validate_havok_xml(xml: &str) -> Result<HavokXmlFacts, ValidationErrors> {
    let root = match parse_xml(xml) {
        Ok(root) => root,
        Err(message) => {
            let mut errors = ValidationErrors::default();
            errors.push("malformed_xml", message);
            return Err(errors);
        }
    };
    let mut errors = ValidationErrors::default();
    let mut facts = HavokXmlFacts::default();

    if root.name != "hkpackfile" {
        errors.push(
            "wrong_xml_root",
            format!("expected hkpackfile root, found {:?}", root.name),
        );
    }
    facts.classversion = root
        .attributes
        .get("classversion")
        .cloned()
        .unwrap_or_default();
    facts.contentsversion = root
        .attributes
        .get("contentsversion")
        .cloned()
        .unwrap_or_default();
    if facts.classversion != "11" {
        errors.push(
            "havok_classversion",
            format!(
                "FO4 XML must use classversion 11, got {:?}",
                facts.classversion
            ),
        );
    }
    if facts.contentsversion != "hk_2014.1.0-r1" {
        errors.push(
            "havok_contentsversion",
            format!(
                "FO4 XML must use hk_2014.1.0-r1 contents, got {:?}",
                facts.contentsversion
            ),
        );
    }

    collect_object_ids(&root, &mut facts.object_ids, &mut errors);
    collect_references(&root, &mut facts.references);
    for reference in &facts.references {
        if !facts.object_ids.contains(reference) {
            errors.push(
                "dangling_hkobject_reference",
                format!("reference {reference} does not name an hkobject in this document"),
            );
        }
    }
    if let Some(toplevel) = root.attributes.get("toplevelobject") {
        if !facts.object_ids.contains(toplevel) {
            errors.push(
                "invalid_toplevelobject",
                format!("toplevelobject {toplevel:?} does not resolve"),
            );
        }
    } else {
        errors.push(
            "missing_toplevelobject",
            "hkpackfile must name its root container with toplevelobject",
        );
    }

    validate_numelements(&root, &mut errors);
    validate_declaration_arrays(&root, &mut facts, &mut errors);
    validate_binding_indices(&root, &facts, &mut errors);

    if errors.is_empty() {
        Ok(facts)
    } else {
        Err(errors)
    }
}

pub fn validate_fo4_havok_xml_signatures(xml: &str) -> Result<(), ValidationErrors> {
    let root = match parse_xml(xml) {
        Ok(root) => root,
        Err(message) => {
            let mut errors = ValidationErrors::default();
            errors.push("malformed_xml", message);
            return Err(errors);
        }
    };
    let mut errors = ValidationErrors::default();
    let mut registry = DescriptorRegistry::for_contents_version("hk_2014.1.0-r1");
    validate_object_signatures(&root, &mut registry, &mut errors);
    errors.finish()
}

fn validate_object_signatures(
    node: &XmlNode,
    registry: &mut DescriptorRegistry,
    errors: &mut ValidationErrors,
) {
    if node.name == "hkobject"
        && let Some(class_name) = node.attributes.get("class")
    {
        let object_name = node
            .attributes
            .get("name")
            .map(String::as_str)
            .unwrap_or("<inline>");
        match registry.get(class_name) {
            Ok(Some(descriptor)) => {
                let expected = parse_signature(&descriptor.signature);
                let actual = node
                    .attributes
                    .get("signature")
                    .and_then(|signature| parse_signature(signature));
                match (expected, actual) {
                    (Some(expected), Some(actual)) if expected == actual => {}
                    (Some(expected), Some(actual)) => errors.push(
                        "havok_signature_mismatch",
                        format!(
                            "hkobject {object_name} class {class_name} declares 0x{actual:08x}, hk2014 descriptor requires 0x{expected:08x}"
                        ),
                    ),
                    (Some(_), None) => errors.push(
                        "invalid_havok_signature",
                        format!(
                            "hkobject {object_name} class {class_name} has no valid hexadecimal signature"
                        ),
                    ),
                    (None, _) => errors.push(
                        "invalid_descriptor_signature",
                        format!(
                            "hk2014 descriptor for class {class_name} has invalid signature {:?}",
                            descriptor.signature
                        ),
                    ),
                }
            }
            Ok(None) => errors.push(
                "unknown_havok_class",
                format!(
                    "hkobject {object_name} uses class {class_name:?}, which is absent from the hk2014 descriptor registry"
                ),
            ),
            Err(source) => errors.push(
                "havok_descriptor_registry",
                format!("failed to load hk2014 descriptor for {class_name}: {source}"),
            ),
        }
    }
    for child in &node.children {
        validate_object_signatures(child, registry, errors);
    }
}

fn parse_signature(value: &str) -> Option<u32> {
    let digits = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))?;
    u32::from_str_radix(digits, 16).ok()
}

fn parse_xml(xml: &str) -> Result<XmlNode, String> {
    let xml = strip_comments(xml)?;
    let mut stack: Vec<XmlNode> = Vec::new();
    let mut root: Option<XmlNode> = None;
    let mut cursor = 0;

    while let Some(relative_open) = xml[cursor..].find('<') {
        let open = cursor + relative_open;
        if let Some(current) = stack.last_mut() {
            current.text.push_str(&xml[cursor..open]);
        } else if !xml[cursor..open].trim().is_empty() {
            return Err("text appears outside the XML root element".to_string());
        }

        let relative_close = xml[open..]
            .find('>')
            .ok_or_else(|| "unterminated XML tag".to_string())?;
        let close = open + relative_close;
        let mut tag = xml[open + 1..close].trim();
        cursor = close + 1;

        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        if let Some(closing_name) = tag.strip_prefix('/') {
            let node = stack
                .pop()
                .ok_or_else(|| format!("unexpected closing tag </{closing_name}>"))?;
            if node.name != closing_name.trim() {
                return Err(format!(
                    "closing tag </{}> does not match <{}>",
                    closing_name.trim(),
                    node.name
                ));
            }
            attach_node(node, &mut stack, &mut root)?;
            continue;
        }

        let self_closing = tag.ends_with('/');
        if self_closing {
            tag = tag[..tag.len() - 1].trim_end();
        }
        let (name, attributes) = parse_start_tag(tag)?;
        let node = XmlNode {
            name,
            attributes,
            text: String::new(),
            children: Vec::new(),
        };
        if self_closing {
            attach_node(node, &mut stack, &mut root)?;
        } else {
            stack.push(node);
        }
    }

    if !xml[cursor..].trim().is_empty() {
        return Err("text appears after the XML root element".to_string());
    }
    if !stack.is_empty() {
        return Err(format!("unclosed XML tag <{}>", stack.last().unwrap().name));
    }
    root.ok_or_else(|| "XML document has no root element".to_string())
}

fn strip_comments(xml: &str) -> Result<String, String> {
    let mut output = String::with_capacity(xml.len());
    let mut cursor = 0;
    while let Some(relative_start) = xml[cursor..].find("<!--") {
        let start = cursor + relative_start;
        output.push_str(&xml[cursor..start]);
        let comment_body = start + 4;
        let relative_end = xml[comment_body..]
            .find("-->")
            .ok_or_else(|| "unterminated XML comment".to_string())?;
        cursor = comment_body + relative_end + 3;
    }
    output.push_str(&xml[cursor..]);
    Ok(output)
}

fn parse_start_tag(tag: &str) -> Result<(String, BTreeMap<String, String>), String> {
    let mut index = 0;
    let bytes = tag.as_bytes();
    while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    if index == 0 {
        return Err("empty XML tag".to_string());
    }
    let name = tag[..index].to_string();
    let mut attributes = BTreeMap::new();

    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index == bytes.len() {
            break;
        }
        let key_start = index;
        while index < bytes.len() && !bytes[index].is_ascii_whitespace() && bytes[index] != b'=' {
            index += 1;
        }
        let key = &tag[key_start..index];
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] != b'=' {
            return Err(format!("attribute {key:?} has no '='"));
        }
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() || !matches!(bytes[index], b'\'' | b'"') {
            return Err(format!("attribute {key:?} has no quoted value"));
        }
        let quote = bytes[index];
        index += 1;
        let value_start = index;
        while index < bytes.len() && bytes[index] != quote {
            index += 1;
        }
        if index == bytes.len() {
            return Err(format!("attribute {key:?} has an unterminated value"));
        }
        let value = tag[value_start..index].to_string();
        index += 1;
        if attributes.insert(key.to_string(), value).is_some() {
            return Err(format!("duplicate attribute {key:?}"));
        }
    }

    Ok((name, attributes))
}

fn attach_node(
    node: XmlNode,
    stack: &mut [XmlNode],
    root: &mut Option<XmlNode>,
) -> Result<(), String> {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    } else if root.replace(node).is_some() {
        return Err("XML document has more than one root element".to_string());
    }
    Ok(())
}

fn collect_object_ids(
    node: &XmlNode,
    object_ids: &mut BTreeSet<String>,
    errors: &mut ValidationErrors,
) {
    if node.name == "hkobject"
        && let Some(name) = node.attributes.get("name")
        && let Some(numeric_id) = name.strip_prefix('#')
    {
        if numeric_id.is_empty()
            || !numeric_id
                .chars()
                .all(|character| character.is_ascii_digit())
        {
            errors.push(
                "invalid_hkobject_id",
                format!("hkobject ID {name:?} must use # followed by decimal digits"),
            );
        }
        if !object_ids.insert(name.clone()) {
            errors.push(
                "duplicate_hkobject_id",
                format!("hkobject ID {name} is declared more than once"),
            );
        }
    }
    for child in &node.children {
        collect_object_ids(child, object_ids, errors);
    }
}

fn collect_references(node: &XmlNode, references: &mut BTreeSet<String>) {
    if node.name != "hkobject" {
        for token in node.text.split_whitespace() {
            let token =
                token.trim_matches(|character: char| matches!(character, '(' | ')' | ',' | ';'));
            if token.starts_with('#')
                && token.len() > 1
                && token[1..]
                    .chars()
                    .all(|character| character.is_ascii_digit())
            {
                references.insert(token.to_string());
            }
        }
    }
    if node.name == "hkpackfile"
        && let Some(reference) = node.attributes.get("toplevelobject")
    {
        references.insert(reference.clone());
    }
    for child in &node.children {
        collect_references(child, references);
    }
}

fn validate_numelements(node: &XmlNode, errors: &mut ValidationErrors) {
    if node.name == "hkparam"
        && let Some(declared) = node.attributes.get("numelements")
    {
        match declared.parse::<usize>() {
            Ok(declared) => {
                let structured_count = node
                    .children
                    .iter()
                    .filter(|child| matches!(child.name.as_str(), "hkobject" | "hkcstring"))
                    .count();
                let actual = if structured_count > 0 {
                    structured_count
                } else {
                    node.text.split_whitespace().count()
                };
                if declared != actual {
                    errors.push(
                        "numelements_mismatch",
                        format!(
                            "hkparam {:?} declares {declared} elements but contains {actual}",
                            node.attributes
                                .get("name")
                                .map(String::as_str)
                                .unwrap_or("")
                        ),
                    );
                }
            }
            Err(_) => errors.push(
                "invalid_numelements",
                format!("numelements value {declared:?} is not an unsigned integer"),
            ),
        }
    }
    for child in &node.children {
        validate_numelements(child, errors);
    }
}

fn validate_declaration_arrays(
    root: &XmlNode,
    facts: &mut HavokXmlFacts,
    errors: &mut ValidationErrors,
) {
    let behavior_strings = objects_by_class(root, "hkbBehaviorGraphStringData");
    let behavior_data = objects_by_class(root, "hkbBehaviorGraphData");
    if behavior_strings.len() != behavior_data.len() {
        errors.push(
            "behavior_data_alignment",
            format!(
                "document has {} behavior string-data objects and {} behavior-data objects",
                behavior_strings.len(),
                behavior_data.len()
            ),
        );
    }

    for (strings, data) in behavior_strings.iter().zip(behavior_data.iter()) {
        let event_names = string_values(strings, "eventNames");
        let variable_names = string_values(strings, "variableNames");
        let property_names = string_values(strings, "characterPropertyNames");
        facts.event_names.extend(event_names.iter().cloned());
        facts.variable_names.extend(variable_names.iter().cloned());
        facts
            .character_property_names
            .extend(property_names.iter().cloned());

        compare_count(
            "event names/event infos",
            event_names.len(),
            param_element_count(data, "eventInfos"),
            errors,
        );
        compare_count(
            "variable names/variable infos",
            variable_names.len(),
            param_element_count(data, "variableInfos"),
            errors,
        );
        compare_count(
            "variable names/variable bounds",
            variable_names.len(),
            param_element_count(data, "variableBounds"),
            errors,
        );
        compare_count(
            "character property names/property infos",
            property_names.len(),
            param_element_count(data, "characterPropertyInfos"),
            errors,
        );

        if let Some(value_set_id) = param_text(data, "variableInitialValues")
            && let Some(value_set) = object_by_id(root, value_set_id)
        {
            compare_count(
                "variable names/initial values",
                variable_names.len(),
                param_element_count(value_set, "wordVariableValues"),
                errors,
            );
        }
    }

    let character_strings = objects_by_class(root, "hkbCharacterStringData");
    let character_data = objects_by_class(root, "hkbCharacterData");
    for (strings, data) in character_strings.iter().zip(character_data.iter()) {
        let property_names = string_values(strings, "characterPropertyNames");
        facts
            .character_property_names
            .extend(property_names.iter().cloned());
        compare_count(
            "character property names/character data infos",
            property_names.len(),
            param_element_count(data, "characterPropertyInfos"),
            errors,
        );
        if let Some(value_set_id) = param_text(data, "characterPropertyValues")
            && let Some(value_set) = object_by_id(root, value_set_id)
        {
            compare_count(
                "character property names/initial values",
                property_names.len(),
                param_element_count(value_set, "wordVariableValues"),
                errors,
            );
        }
    }
}

fn validate_binding_indices(root: &XmlNode, facts: &HavokXmlFacts, errors: &mut ValidationErrors) {
    for binding_set in objects_by_class(root, "hkbVariableBindingSet") {
        let Some(bindings) = param(binding_set, "bindings") else {
            continue;
        };
        for binding in bindings
            .children
            .iter()
            .filter(|child| child.name == "hkobject")
        {
            let binding_type = param_text(binding, "bindingType").unwrap_or("");
            let Some(index_text) = param_text(binding, "variableIndex") else {
                errors.push("binding_index", "variable binding has no variableIndex");
                continue;
            };
            let Ok(index) = index_text.parse::<usize>() else {
                errors.push(
                    "binding_index",
                    format!("variableIndex {index_text:?} is not a nonnegative integer"),
                );
                continue;
            };
            let count = if binding_type == "BINDING_TYPE_CHARACTER_PROPERTY" {
                facts.character_property_names.len()
            } else {
                facts.variable_names.len()
            };
            if index >= count {
                errors.push(
                    "binding_index",
                    format!(
                        "{binding_type} index {index} is out of range for {count} declarations"
                    ),
                );
            }
        }
    }
}

fn compare_count(
    label: &str,
    expected: usize,
    actual: Option<usize>,
    errors: &mut ValidationErrors,
) {
    match actual {
        Some(actual) if actual == expected => {}
        Some(actual) => errors.push(
            "declaration_array_alignment",
            format!("{label} count differs: {expected} versus {actual}"),
        ),
        None => errors.push(
            "missing_declaration_array",
            format!("missing array required for {label}"),
        ),
    }
}

fn objects_by_class<'a>(node: &'a XmlNode, class_name: &str) -> Vec<&'a XmlNode> {
    let mut objects = Vec::new();
    collect_objects_by_class(node, class_name, &mut objects);
    objects
}

fn collect_objects_by_class<'a>(
    node: &'a XmlNode,
    class_name: &str,
    objects: &mut Vec<&'a XmlNode>,
) {
    if node.name == "hkobject"
        && node.attributes.get("class").map(String::as_str) == Some(class_name)
    {
        objects.push(node);
    }
    for child in &node.children {
        collect_objects_by_class(child, class_name, objects);
    }
}

fn object_by_id<'a>(node: &'a XmlNode, id: &str) -> Option<&'a XmlNode> {
    if node.name == "hkobject" && node.attributes.get("name").map(String::as_str) == Some(id) {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| object_by_id(child, id))
}

fn param<'a>(node: &'a XmlNode, name: &str) -> Option<&'a XmlNode> {
    node.children.iter().find(|child| {
        child.name == "hkparam" && child.attributes.get("name").map(String::as_str) == Some(name)
    })
}

fn param_text<'a>(node: &'a XmlNode, name: &str) -> Option<&'a str> {
    param(node, name).map(|parameter| parameter.text.trim())
}

fn param_element_count(node: &XmlNode, name: &str) -> Option<usize> {
    param(node, name)?
        .attributes
        .get("numelements")?
        .parse()
        .ok()
}

fn string_values(node: &XmlNode, name: &str) -> Vec<String> {
    param(node, name)
        .into_iter()
        .flat_map(|parameter| parameter.children.iter())
        .filter(|child| child.name == "hkcstring")
        .map(|child| child.text.trim().to_string())
        .collect()
}

#[allow(dead_code)]
fn variable_type(value: &str) -> Option<VariableType> {
    match value {
        "VARIABLE_TYPE_BOOL" => Some(VariableType::Bool),
        "VARIABLE_TYPE_INT32" => Some(VariableType::Int32),
        "VARIABLE_TYPE_REAL" => Some(VariableType::Real),
        _ => None,
    }
}
