use indexmap::IndexMap;
use papyrus_core::pex::PexFilePayload;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::LazyLock;

macro_rules! pattern {
    ($name:ident, $value:literal) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($value).unwrap());
    };
}
pattern!(
    MEMBER,
    r"(?i)^\s*(?:(?P<ret>[A-Za-z_][\w:]*(?:\[\])*)\s+)?(?P<kind>Function|Event)\s+(?P<name>[A-Za-z_]\w*(?::[A-Za-z_]\w*)*(?:\.[A-Za-z_]\w*)?)\s*\("
);
pattern!(
    CANDIDATE,
    r"(?i)^\s*(?:\S+\s+)?(?:Function|Event)\s+[A-Za-z_][\w:.]*\s*\("
);
pattern!(
    PSEUDO,
    r"(?i)^\s*(?:[A-Za-z_][\w\[\]]*\s+)?Function\s+::remote_(?P<name>[A-Za-z_]\w*)\s*\("
);
pattern!(MEMBER_END, r"(?i)^\s*End(?:Function|Event)\b");
pattern!(FUNCTION_END, r"(?i)^\s*EndFunction\b");
pattern!(
    STATE,
    r"(?i)^(?P<indent>\s*)(?P<auto>Auto\s+)?State\s+(?P<name>[A-Za-z_]\w*)\b"
);
pattern!(STATE_END, r"(?i)^\s*EndState\b");
pattern!(
    RENAME,
    r"(?i)^\s*;\s*@state-rename\s+(?P<old>[A-Za-z_]\w*)\s+(?P<new>[A-Za-z_]\w*)\s*$"
);
pattern!(
    DROP_MEMBER,
    r"(?i)^\s*;\s*@drop-member\s+(?P<name>[A-Za-z_]\w*(?:\.[A-Za-z_]\w*)?)\s*$"
);
pattern!(
    DROP_PROPERTY,
    r"(?i)^\s*;\s*@drop-property\s+(?P<name>[A-Za-z_]\w*)\s*$"
);
pattern!(
    EXTENDS,
    r"(?i)^\s*;\s*@extends\s+(?P<name>[A-Za-z_]\w*(?::[A-Za-z_]\w*)*)\s*$"
);
pattern!(
    HEADER_PARENT,
    r"(?i)^(?P<head>\s*Scriptname\s+[A-Za-z_][\w:]*)(?:\s+Extends\s+[A-Za-z_][\w:]*)?(?P<tail>.*)$"
);
pattern!(
    PROPERTY,
    r"(?i)^\s*(?P<type>[A-Za-z_][\w:]*(?:\[\])*)\s+Property\s+(?P<name>[A-Za-z_]\w*)\b"
);
pattern!(
    VARIABLE,
    r"(?i)^\s*(?P<type>[A-Za-z_][\w:]*(?:\[\])*)\s+(?P<name>[A-Za-z_]\w*)\b"
);
pattern!(AUTO, r"(?i)\bAuto\b");
pattern!(
    AUTO_TAIL,
    r"(?i)\bAuto\b(?:\s+(?:Const|Mandatory|Hidden|Conditional))*\s*(?:;.*)?$"
);
pattern!(HEADER, r"(?i)^\s*Scriptname\s+");
pattern!(
    DECLARATION,
    r"(?im)^\s*Scriptname\s+(?P<name>[A-Za-z_][\w:]*)\b"
);

type Member = (String, String, usize, usize);
type State = (String, usize, usize);

pub fn key(name: &str) -> String {
    name.replace(['\\', '/'], ":").trim().to_lowercase()
}

fn lines(source: &str) -> Vec<String> {
    source.lines().map(str::to_owned).collect()
}

pub fn declaration(source: &str) -> Option<String> {
    DECLARATION.captures(source).map(|c| c["name"].to_owned())
}

fn top_members(lines: &[String]) -> Vec<Member> {
    let mut members = Vec::new();
    let mut depth = 0usize;
    let mut i = 0;
    while i < lines.len() {
        if STATE_END.is_match(&lines[i]) {
            depth = depth.saturating_sub(1);
            i += 1;
            continue;
        }
        if let Some(c) = MEMBER.captures(&lines[i]) {
            let end = (i + 1..lines.len())
                .find(|&j| MEMBER_END.is_match(&lines[j]))
                .unwrap_or(lines.len());
            if depth == 0 {
                members.push((
                    c["kind"].to_lowercase(),
                    c["name"].to_lowercase(),
                    i,
                    end.min(lines.len() - 1),
                ));
            }
            i = end + 1;
            continue;
        }
        if STATE.is_match(&lines[i]) {
            depth += 1;
        }
        i += 1;
    }
    members
}

fn states(lines: &[String]) -> Result<Vec<State>, String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if let Some(c) = STATE.captures(&lines[i]) {
            let end = (i + 1..lines.len())
                .find(|&j| STATE_END.is_match(&lines[j]))
                .ok_or_else(|| format!("unterminated Papyrus state {}", &c["name"]))?;
            out.push((c["name"].to_lowercase(), i, end));
            i = end + 1;
        } else {
            i += 1;
        }
    }
    Ok(out)
}

fn state_members(lines: &[String], start: usize, end: usize) -> Result<Vec<Member>, String> {
    let mut out = Vec::new();
    let mut i = start + 1;
    while i < end {
        if let Some(c) = MEMBER.captures(&lines[i]) {
            let stop = (i + 1..end)
                .find(|&j| MEMBER_END.is_match(&lines[j]))
                .ok_or_else(|| format!("unterminated Papyrus {} {}", &c["kind"], &c["name"]))?;
            out.push((c["kind"].to_lowercase(), c["name"].to_lowercase(), i, stop));
            i = stop + 1;
        } else {
            i += 1;
        }
    }
    Ok(out)
}

fn pseudo_members(lines: &[String], range: Option<(usize, usize)>) -> Vec<(String, usize, usize)> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let (mut i, end) = range.map_or((0, lines.len()), |(a, b)| (a + 1, b));
    while i < end {
        if range.is_none() && STATE_END.is_match(&lines[i]) {
            depth = depth.saturating_sub(1);
            i += 1;
            continue;
        }
        if let Some(c) = PSEUDO.captures(&lines[i]) {
            let stop = (i + 1..end)
                .find(|&j| FUNCTION_END.is_match(&lines[j]))
                .unwrap_or(end);
            if depth == 0 && stop < end {
                out.push((c["name"].to_lowercase(), i, stop));
            }
            i = stop + 1;
            continue;
        }
        if range.is_none() && STATE.is_match(&lines[i]) {
            depth += 1;
        }
        i += 1;
    }
    out
}

fn recognized(lines: &[String]) -> Result<(), String> {
    let mut starts: HashSet<usize> = top_members(lines).into_iter().map(|x| x.2).collect();
    for (_, a, b) in states(lines)? {
        starts.extend(state_members(lines, a, b)?.into_iter().map(|x| x.2));
    }
    for (i, line) in lines.iter().enumerate() {
        if !line.trim_start().starts_with(';') && CANDIDATE.is_match(line) && !starts.contains(&i) {
            return Err(format!(
                "unrecognized Papyrus patch member declaration at line {}: {}",
                i + 1,
                line.trim()
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Patch {
    pub members: Vec<(String, String, String, Vec<String>)>,
    pub renames: Vec<(String, String)>,
    pub drop_members: BTreeSet<String>,
    pub drop_properties: BTreeSet<String>,
    /// `; @extends <Parent>` — FO76 attached some scripts to forms FO4 cannot carry
    /// (the player's NPC_ record), so the carrier changes type and the header must follow.
    pub extends: Option<String>,
}

impl Patch {
    pub fn parse(source: &str) -> Result<Self, String> {
        let lines = lines(source);
        recognized(&lines)?;
        let renames: Vec<_> = lines
            .iter()
            .filter_map(|l| {
                RENAME
                    .captures(l)
                    .map(|c| (c["old"].to_owned(), c["new"].to_owned()))
            })
            .collect();
        let rename_map: HashMap<_, _> = renames
            .iter()
            .map(|(a, b)| (a.to_lowercase(), b.to_lowercase()))
            .collect();
        let mut members = Vec::new();
        let mut seen = HashSet::new();
        let mut add = |state: &str, member: Member| -> Result<(), String> {
            let (kind, name, a, b) = member;
            if !seen.insert((state.to_owned(), kind.clone(), name.clone())) {
                return Err(format!(
                    "duplicate Papyrus patch member in {}: {kind} {name}",
                    if state.is_empty() {
                        "default state"
                    } else {
                        state
                    }
                ));
            }
            members.push((state.to_owned(), kind, name, lines[a..=b].to_vec()));
            Ok(())
        };
        for member in top_members(&lines) {
            add("", member)?;
        }
        for (state, a, b) in states(&lines)? {
            let target = rename_map.get(&state).unwrap_or(&state);
            for member in state_members(&lines, a, b)? {
                add(target, member)?;
            }
        }
        let parents: Vec<_> = lines
            .iter()
            .filter_map(|l| EXTENDS.captures(l).map(|c| c["name"].to_owned()))
            .collect();
        if parents.len() > 1 {
            return Err(format!(
                "Papyrus patch declares more than one @extends: {}",
                parents.join(", ")
            ));
        }
        Ok(Self {
            members,
            renames,
            drop_members: lines
                .iter()
                .filter_map(|l| DROP_MEMBER.captures(l).map(|c| c["name"].to_lowercase()))
                .collect(),
            drop_properties: lines
                .iter()
                .filter_map(|l| DROP_PROPERTY.captures(l).map(|c| c["name"].to_lowercase()))
                .collect(),
            extends: parents.into_iter().next(),
        })
    }

    pub fn functional(&self) -> bool {
        !self.members.is_empty() || self.directive_count() != 0
    }

    pub fn directive_count(&self) -> usize {
        self.renames.len()
            + self.drop_members.len()
            + self.drop_properties.len()
            + usize::from(self.extends.is_some())
    }
}

fn excluded(lines: &[String]) -> Result<HashSet<usize>, String> {
    let mut excluded = HashSet::new();
    for (_, a, b) in states(lines)? {
        excluded.extend(a..=b);
    }
    for (_, _, a, b) in top_members(lines) {
        excluded.extend(a..=b);
    }
    Ok(excluded)
}

fn drop_properties(
    lines: &mut Vec<String>,
    names: &BTreeSet<String>,
    strict_tail: bool,
) -> Result<(), String> {
    if names.is_empty() {
        return Ok(());
    }
    let excluded = excluded(lines)?;
    for i in (0..lines.len()).rev() {
        if excluded.contains(&i) {
            continue;
        }
        if let Some(c) = PROPERTY.captures(&lines[i]) {
            if names.contains(&c["name"].to_lowercase())
                && (if strict_tail { &*AUTO_TAIL } else { &*AUTO }).is_match(&lines[i])
            {
                lines.remove(i);
            }
        }
    }
    Ok(())
}

fn rename_states(lines: &mut [String], directives: &[(String, String)]) -> Result<(), String> {
    let mut seen = HashSet::new();
    for (old, new) in directives {
        let old_key = old.to_lowercase();
        let new_key = new.to_lowercase();
        if !seen.insert(old_key.clone()) {
            return Err(format!("duplicate Papyrus state rename for {old}"));
        }
        let spans = states(lines)?;
        let found: Vec<_> = spans.iter().filter(|x| x.0 == old_key).collect();
        if found.len() != 1 {
            return Err(format!(
                "Papyrus state rename requires exactly one '{old}' state; found {}",
                found.len()
            ));
        }
        if old_key != new_key && spans.iter().any(|x| x.0 == new_key) {
            return Err(format!("Papyrus state rename target already exists: {new}"));
        }
        let declaration = found[0].1;
        let c = STATE.captures(&lines[declaration]).unwrap();
        let span = c.name("name").unwrap();
        lines[declaration] = format!(
            "{}{new}{}",
            &lines[declaration][..span.start()],
            &lines[declaration][span.end()..]
        );
        let goto = Regex::new(&format!(
            r#"(?i)(\b(?:Self\.)?GoToState\s*\(\s*"){}("\s*\))"#,
            regex::escape(old)
        ))
        .unwrap();
        for line in lines.iter_mut() {
            *line = goto
                .replace_all(line, |c: &regex::Captures| {
                    format!("{}{new}{}", &c[1], &c[2])
                })
                .into_owned();
        }
    }
    Ok(())
}

fn header_index(lines: &[String]) -> Result<usize, String> {
    let headers: Vec<_> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| HEADER.is_match(l))
        .map(|(i, _)| i)
        .collect();
    match headers.as_slice() {
        [index] => Ok(*index),
        _ => Err(format!(
            "Papyrus @extends requires exactly one Scriptname header; found {}",
            headers.len()
        )),
    }
}

fn header_parent(line: &str) -> Option<String> {
    Regex::new(r"(?i)\bExtends\s+(?P<parent>[A-Za-z_][\w:]*)")
        .unwrap()
        .captures(line)
        .map(|c| c["parent"].to_owned())
}

fn set_parent(lines: &mut [String], parent: &str) -> Result<(), String> {
    let index = header_index(lines)?;
    let c = HEADER_PARENT
        .captures(&lines[index])
        .ok_or_else(|| format!("unparseable Scriptname header: {}", lines[index].trim()))?;
    lines[index] = format!("{} Extends {parent}{}", &c["head"], &c["tail"]);
    Ok(())
}

fn pseudo_key(kind: &str, name: &str) -> Option<String> {
    if kind == "event" && name.contains('.') {
        Some(name.replace('.', "_"))
    } else {
        None
    }
}

pub fn merge(skeleton: &str, source: &str) -> Result<String, String> {
    let patch = Patch::parse(source)?;
    let patch_lines = lines(source);
    let patch_states = states(&patch_lines)?;
    if !patch.functional() && patch_states.is_empty() {
        return Ok(skeleton.to_owned());
    }
    let mut lines = lines(skeleton);
    if let Some(parent) = &patch.extends {
        set_parent(&mut lines, parent)?;
    }
    drop_properties(&mut lines, &patch.drop_properties, false)?;
    let mut i = 0;
    let mut remove = Vec::new();
    while i < lines.len() {
        if let Some(c) = MEMBER.captures(&lines[i]) {
            let stop = (i + 1..lines.len())
                .find(|&j| MEMBER_END.is_match(&lines[j]))
                .unwrap_or(lines.len());
            if patch.drop_members.contains(&c["name"].to_lowercase()) {
                remove.push((i, stop.min(lines.len() - 1)));
            }
            i = stop + 1;
        } else {
            i += 1;
        }
    }
    for (a, b) in remove.into_iter().rev() {
        lines.drain(a..=b);
    }
    rename_states(&mut lines, &patch.renames)?;
    let rename_map: HashMap<_, _> = patch
        .renames
        .iter()
        .map(|(a, b)| (a.to_lowercase(), b.to_lowercase()))
        .collect();
    let mut seen_states = HashSet::new();
    for (state, pa, pb) in patch_states {
        let target = rename_map.get(&state).unwrap_or(&state);
        if !seen_states.insert(target.clone()) {
            return Err(format!("duplicate Papyrus state patch for {target}"));
        }
        let members = state_members(&patch_lines, pa, pb)?;
        if members.is_empty() {
            continue;
        }
        let target_states: Vec<_> = states(&lines)?
            .into_iter()
            .filter(|x| &x.0 == target)
            .collect();
        if target_states.len() != 1 {
            return Err(format!(
                "Papyrus named-state patch requires exactly one '{target}' state; found {}",
                target_states.len()
            ));
        }
        let (_, a, b) = target_states[0];
        let blocks: IndexMap<_, _> = members
            .into_iter()
            .map(|(k, n, a, b)| ((k, n), patch_lines[a..=b].to_vec()))
            .collect();
        let mut replacements = Vec::new();
        let mut matched = HashSet::new();
        for (kind, name, a, b) in state_members(&lines, a, b)? {
            let key = (kind, name);
            if let Some(block) = blocks.get(&key) {
                replacements.push((a, b, block.clone()));
                matched.insert(key);
            }
        }
        for (pseudo, a, b) in pseudo_members(&lines, Some((a, b))) {
            if let Some((key, block)) = blocks
                .iter()
                .find(|((k, n), _)| pseudo_key(k, n).as_ref() == Some(&pseudo))
            {
                replacements.push((a, b, block.clone()));
                matched.insert(key.clone());
            }
        }
        replacements.sort_by_key(|x| std::cmp::Reverse((x.0, x.1)));
        for (a, b, block) in replacements {
            lines.splice(a..=b, block);
        }
        let insert_at = states(&lines)?
            .into_iter()
            .find(|x| &x.0 == target)
            .unwrap()
            .2;
        let mut additions = Vec::new();
        for (key, block) in blocks {
            if !matched.contains(&key) {
                if !additions.is_empty() {
                    additions.push(String::new());
                }
                additions.extend(block);
            }
        }
        if !additions.is_empty() {
            additions.push(String::new());
            lines.splice(insert_at..insert_at, additions);
        }
    }
    let blocks: IndexMap<_, _> = patch
        .members
        .iter()
        .filter(|x| x.0.is_empty())
        .map(|(_, k, n, b)| ((k.clone(), n.clone()), b.join("\n")))
        .collect();
    let mut remove: Vec<_> = top_members(&lines)
        .into_iter()
        .filter(|(k, n, _, _)| blocks.contains_key(&(k.clone(), n.clone())))
        .map(|(_, _, a, b)| (a, b))
        .collect();
    for (pseudo, a, b) in pseudo_members(&lines, None) {
        if blocks
            .keys()
            .any(|(k, n)| pseudo_key(k, n).as_ref() == Some(&pseudo))
        {
            remove.push((a, b));
        }
    }
    remove.sort_by_key(|x| std::cmp::Reverse(*x));
    for (a, b) in remove {
        lines.drain(a..=b);
    }
    let mut merged = lines.join("\n").trim_end().to_owned();
    if !blocks.is_empty() {
        merged.push_str("\n\n");
        merged.push_str(&blocks.into_values().collect::<Vec<_>>().join("\n\n"));
    }
    merged.push('\n');
    validate_merged(&patch, &merged)?;
    Ok(merged)
}

pub fn validate_merged(patch: &Patch, source: &str) -> Result<(), String> {
    let lines = lines(source);
    if let Some(parent) = &patch.extends {
        let found = header_parent(&lines[header_index(&lines)?]);
        if !found.is_some_and(|f| f.eq_ignore_ascii_case(parent)) {
            return Err(format!(
                "Papyrus patch delivery expected the script to extend {parent}"
            ));
        }
    }
    let top = top_members(&lines);
    let spans = states(&lines)?;
    for (state, kind, name, expected) in &patch.members {
        let members = if state.is_empty() {
            top.clone()
        } else {
            let found: Vec<_> = spans.iter().filter(|s| &s.0 == state).collect();
            if found.len() != 1 {
                return Err(format!(
                    "Papyrus patch delivery expected one '{state}' state; found {}",
                    found.len()
                ));
            }
            state_members(&lines, found[0].1, found[0].2)?
        };
        let found: Vec<_> = members
            .iter()
            .filter(|(k, n, _, _)| k == kind && n == name)
            .collect();
        let location = if state.is_empty() {
            "default state"
        } else {
            state
        };
        if found.len() != 1 {
            return Err(format!(
                "Papyrus patch delivery expected exactly one {kind} {name} in {location}; found {}",
                found.len()
            ));
        }
        if &lines[found[0].2..=found[0].3] != expected {
            return Err(format!(
                "Papyrus patch delivery changed {kind} {name} in {location}"
            ));
        }
    }
    let mut names: HashSet<_> = top.into_iter().map(|x| x.1).collect();
    for (_, a, b) in &spans {
        names.extend(state_members(&lines, *a, *b)?.into_iter().map(|x| x.1));
    }
    let retained: Vec<_> = patch
        .drop_members
        .iter()
        .filter(|n| names.contains(*n))
        .cloned()
        .collect();
    if !retained.is_empty() {
        return Err(format!(
            "Papyrus patch delivery retained dropped member(s): {}",
            retained.join(", ")
        ));
    }
    let props: HashSet<_> = lines
        .iter()
        .filter_map(|l| PROPERTY.captures(l).map(|c| c["name"].to_lowercase()))
        .collect();
    let retained: Vec<_> = patch
        .drop_properties
        .iter()
        .filter(|n| props.contains(*n))
        .cloned()
        .collect();
    if !retained.is_empty() {
        return Err(format!(
            "Papyrus patch delivery retained dropped property(s): {}",
            retained.join(", ")
        ));
    }
    for (old, new) in &patch.renames {
        let old_key = old.to_lowercase();
        let new_key = new.to_lowercase();
        if old_key != new_key && spans.iter().any(|s| s.0 == old_key) {
            return Err(format!(
                "Papyrus patch delivery retained renamed state {old}"
            ));
        }
        let goto = Regex::new(&format!(
            r#"(?i)\b(?:Self\.)?GoToState\s*\(\s*"{}"\s*\)"#,
            regex::escape(old)
        ))
        .unwrap();
        if old_key != new_key && lines.iter().any(|l| goto.is_match(l)) {
            return Err(format!(
                "Papyrus patch delivery retained renamed state target {old}"
            ));
        }
        let count = spans.iter().filter(|s| s.0 == new_key).count();
        if count != 1 {
            return Err(format!(
                "Papyrus patch delivery expected exactly one renamed state {new}; found {count}"
            ));
        }
    }
    Ok(())
}

pub fn validate_compiled(patch: &Patch, pex: &PexFilePayload) -> Result<(), String> {
    let mut members = HashMap::new();
    let mut names = HashSet::new();
    let mut states = HashMap::new();
    let mut props = HashSet::new();
    for object in &pex.objects {
        if let Some(parent) = &patch.extends {
            if !object.parent.eq_ignore_ascii_case(parent) {
                return Err(format!(
                    "compiled PEX extends {}, expected {parent}",
                    object.parent
                ));
            }
        }
        props.extend(object.variables.iter().map(|v| v.name.to_lowercase()));
        props.extend(object.properties.iter().map(|v| v.name.to_lowercase()));
        for state in &object.states {
            let name = state.name.to_lowercase();
            *states.entry(name.clone()).or_insert(0usize) += 1;
            for function in &state.functions {
                let function = function.name.to_lowercase();
                *members
                    .entry((name.clone(), function.clone()))
                    .or_insert(0usize) += 1;
                names.insert(function);
            }
        }
    }
    let mut missing = Vec::new();
    for (state, kind, name, _) in &patch.members {
        let compiled =
            pseudo_key(kind, name).map_or_else(|| name.clone(), |p| format!("::remote_{p}"));
        let count = members
            .get(&(state.clone(), compiled))
            .copied()
            .unwrap_or(0);
        if count != 1 {
            missing.push(format!(
                "{kind} {name} in {} (found {count})",
                if state.is_empty() {
                    "default state"
                } else {
                    state
                }
            ));
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "compiled PEX did not retain Papyrus patch member(s): {}",
            missing.into_iter().take(5).collect::<Vec<_>>().join("; ")
        ));
    }
    let retained: Vec<_> = patch
        .drop_members
        .iter()
        .filter(|n| names.contains(*n))
        .cloned()
        .collect();
    if !retained.is_empty() {
        return Err(format!(
            "compiled PEX retained dropped Papyrus member(s): {}",
            retained.join(", ")
        ));
    }
    let retained: Vec<_> = patch
        .drop_properties
        .iter()
        .filter(|n| props.contains(*n))
        .cloned()
        .collect();
    if !retained.is_empty() {
        return Err(format!(
            "compiled PEX retained dropped Papyrus property(s): {}",
            retained.join(", ")
        ));
    }
    for (old, new) in &patch.renames {
        if !old.eq_ignore_ascii_case(new) && states.contains_key(&old.to_lowercase()) {
            return Err(format!("compiled PEX retained renamed Papyrus state {old}"));
        }
        let count = states.get(&new.to_lowercase()).copied().unwrap_or(0);
        if count != 1 {
            return Err(format!(
                "compiled PEX expected exactly one renamed Papyrus state {new}; found {count}"
            ));
        }
    }
    Ok(())
}

static RULES: LazyLock<serde_json::Value> =
    LazyLock::new(|| serde_json::from_str(include_str!("script_port_rules.json")).unwrap());

pub fn additions(source_game: &str, target_game: &str) -> Vec<String> {
    RULES["additions"][format!(
        "{}:{}",
        source_game.to_lowercase(),
        target_game.to_lowercase()
    )]
    .as_array()
    .map(|v| {
        v.iter()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect()
    })
    .unwrap_or_default()
}

pub fn augment(script: &str, source: &str) -> Result<String, String> {
    let key = key(script);
    let mut source = source.to_owned();
    if key == "defaultquestencounterwavescript" {
        source = Regex::new(r"(?im)^(\s*)curvetable(\s+(?:PeakDifficultyCurve|SubwaveTimeShortCurve|SubwaveTimeCurve)\b)").unwrap().replace_all(&source,"${1}Form${2}").into_owned();
    } else if key == "fragments:quests:qf_mtnm01_mayhem_0009732e" {
        source = Regex::new(r"(?im)^(\s*)perkcard(\s+Property\s+CannibalCard\b)")
            .unwrap()
            .replace_all(&source, "${1}Form${2}")
            .into_owned();
    } else if matches!(
        key.as_str(),
        "topicinfobuydrinks" | "quests:_default:topicinfoexchangeitemsorcurrency"
    ) {
        // FO76's Currency type has no FO4 form, so these bound to None. Every
        // binding is caps (CNCY 00000F), which converts to FO4's Caps001 MISC.
        source = Regex::new(
            r"(?im)^(\s*)currency(\s+Property\s+(?:PaymentType|PlayerCurrency_Type|NPCCurrency_Type)\b)",
        )
        .unwrap()
        .replace_all(&source, "${1}MiscObject${2}")
        .into_owned();
    }
    let empty = Vec::new();
    let variables = RULES["_FO76_TO_FO4_SCRIPT_VARIABLE_ADDITIONS"][&key]
        .as_array()
        .unwrap_or(&empty);
    let properties = RULES["_FO76_TO_FO4_SCRIPT_PROPERTY_ADDITIONS"][&key]
        .as_array()
        .unwrap_or(&empty);
    let removals = RULES["_FO76_TO_FO4_SCRIPT_PROPERTY_REMOVALS"][&key]
        .as_array()
        .unwrap_or(&empty);
    if variables.is_empty() && properties.is_empty() && removals.is_empty() {
        return Ok(source);
    }
    let mut lines = lines(&source);
    drop_properties(
        &mut lines,
        &removals
            .iter()
            .filter_map(|x| x.as_str().map(str::to_lowercase))
            .collect(),
        true,
    )?;
    let headers: Vec<_> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| HEADER.is_match(l))
        .map(|(i, _)| i)
        .collect();
    if headers.len() != 1 {
        return Err(format!(
            "Papyrus skeleton augmentation requires exactly one Scriptname header; found {} for {script}",
            headers.len()
        ));
    }
    let mut insert_at = headers[0] + 1;
    for (property, entries) in [(false, variables), (true, properties)] {
        for entry in entries {
            let ty = entry[0].as_str().unwrap();
            let name = entry[1].as_str().unwrap();
            let name_key = name.to_lowercase();
            let collisions: Vec<_> = top_members(&lines)
                .into_iter()
                .filter(|x| x.1 == name_key)
                .map(|x| lines[x.2].clone())
                .collect();
            if !collisions.is_empty() {
                return Err(format!(
                    "conflicting Papyrus member for {name} in {script}: {}",
                    collisions.join(", ")
                ));
            }
            let excluded = excluded(&lines)?;
            let mut vars = Vec::new();
            let mut props = Vec::new();
            for (i, line) in lines.iter().enumerate() {
                if excluded.contains(&i) {
                    continue;
                }
                if let Some(c) = PROPERTY.captures(line) {
                    if c["name"].to_lowercase() == name_key {
                        props.push((c["type"].to_owned(), line.clone()));
                        continue;
                    }
                }
                if let Some(c) = VARIABLE.captures(line) {
                    if c["name"].to_lowercase() == name_key {
                        vars.push((c["type"].to_owned(), line.clone()));
                    }
                }
            }
            let (wrong, right, wrong_kind, right_kind) = if property {
                (&vars, &props, "declaration", "property")
            } else {
                (&props, &vars, "property", "declaration")
            };
            if !wrong.is_empty() {
                return Err(format!(
                    "conflicting Papyrus {wrong_kind} for {name} in {script}: {}",
                    wrong
                        .iter()
                        .map(|x| x.1.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if !right.is_empty() {
                if right.len() != 1 || !right[0].0.eq_ignore_ascii_case(ty) {
                    return Err(format!(
                        "conflicting Papyrus {right_kind} for {name} in {script}: {}",
                        right
                            .iter()
                            .map(|x| x.1.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                continue;
            }
            lines.insert(
                insert_at,
                if property {
                    entry[2].as_str().unwrap().to_owned()
                } else {
                    format!("{ty} {name}")
                },
            );
            insert_at += 1;
        }
    }
    let mut result = lines.join("\n");
    if source.ends_with(['\n', '\r']) {
        result.push('\n');
    }
    Ok(result)
}

#[pyo3::pyfunction(name = "script_patch_merge")]
fn merge_py(py: pyo3::Python<'_>, skeleton: &str, patch: &str) -> pyo3::PyResult<String> {
    py.detach(|| merge(skeleton, patch))
        .map_err(pyo3::exceptions::PyValueError::new_err)
}

#[pyo3::pyfunction(name = "script_patch_augment")]
fn augment_py(py: pyo3::Python<'_>, script: &str, source: &str) -> pyo3::PyResult<String> {
    py.detach(|| augment(script, source))
        .map_err(pyo3::exceptions::PyValueError::new_err)
}

#[pyo3::pyfunction(name = "script_patch_members")]
fn members_py(py: pyo3::Python<'_>, source: &str) -> Vec<Member> {
    py.detach(|| top_members(&lines(source)))
}

#[pyo3::pyfunction(name = "script_patch_analyze")]
fn analyze_py(py: pyo3::Python<'_>, source: &str) -> pyo3::PyResult<String> {
    py.detach(|| {
        Patch::parse(source)
            .and_then(|patch| serde_json::to_string(&patch).map_err(|e| e.to_string()))
    })
    .map_err(pyo3::exceptions::PyValueError::new_err)
}

#[pyo3::pyfunction(name = "script_patch_validate_compiled")]
fn validate_compiled_py(py: pyo3::Python<'_>, path: &str, source: &str) -> pyo3::PyResult<()> {
    py.detach(|| {
        let patch = Patch::parse(source)?;
        let pex = papyrus_core::pex::parse_pex_file(path)?;
        validate_compiled(&patch, &pex)
    })
    .map_err(pyo3::exceptions::PyValueError::new_err)
}

#[pyo3::pyfunction(name = "script_addition_names")]
fn additions_py(py: pyo3::Python<'_>, source: &str, target: &str) -> Vec<String> {
    py.detach(|| additions(source, target))
}

#[pyo3::pyfunction(name = "script_addition_sources")]
fn addition_sources_py(
    py: pyo3::Python<'_>,
    root: &str,
    source: &str,
    target: &str,
) -> Vec<(String, String, String)> {
    py.detach(|| {
        additions(source, target)
            .into_iter()
            .filter_map(|name| {
                let path = std::path::Path::new(root)
                    .join(format!(
                        "{}_{}",
                        source.to_lowercase(),
                        target.to_lowercase()
                    ))
                    .join(crate::script_conversion::relative_path(&name, "psc"));
                path.is_file()
                    .then(|| (key(&name), name, path.to_string_lossy().into_owned()))
            })
            .collect()
    })
}

#[pyo3::pyfunction(name = "script_requires_addition")]
fn requires_addition_py(py: pyo3::Python<'_>, name: &str, source: &str, target: &str) -> bool {
    py.detach(|| {
        additions(source, target)
            .iter()
            .any(|candidate| key(candidate) == key(name))
    })
}

#[pyo3::pyfunction(name = "script_patch_source")]
fn patch_source_py(py: pyo3::Python<'_>, root: &str, name: &str) -> pyo3::PyResult<Option<String>> {
    py.detach(|| -> Result<Option<String>, String> {
        let path =
            std::path::Path::new(root).join(crate::script_conversion::relative_path(name, "psc"));
        if !path.is_file() {
            return Ok(None);
        }
        std::fs::read_to_string(path)
            .map(|s| Some(s.replace("\r\n", "\n").replace('\r', "\n")))
            .map_err(|e| e.to_string())
    })
    .map_err(pyo3::exceptions::PyIOError::new_err)
}

#[pyo3::pyfunction(name = "script_addition_install")]
fn install_py(py: pyo3::Python<'_>, root: &str, name: &str, path: &str) -> pyo3::PyResult<String> {
    py.detach(|| -> Result<String, String> {
        crate::script_conversion::install_addition(
            std::path::Path::new(root),
            name,
            std::path::Path::new(path),
        )?;
        Ok(std::path::Path::new(root)
            .join("Scripts/Source/User")
            .join(crate::script_conversion::relative_path(name, "psc"))
            .to_string_lossy()
            .into_owned())
    })
    .map_err(pyo3::exceptions::PyValueError::new_err)
}

#[pyo3::pyfunction(name = "script_patch_validate_merged")]
fn validate_merged_py(py: pyo3::Python<'_>, source: &str, merged: &str) -> pyo3::PyResult<()> {
    py.detach(|| validate_merged(&Patch::parse(source)?, merged))
        .map_err(pyo3::exceptions::PyValueError::new_err)
}

#[pyo3::pyfunction(name = "script_toolchain_assert_healthy")]
fn healthy_py(py: pyo3::Python<'_>, resolutions: &str) -> pyo3::PyResult<()> {
    py.detach(|| {
        let resolutions: Vec<crate::script_conversion::Resolution> =
            serde_json::from_str(resolutions).map_err(|e| e.to_string())?;
        crate::script_conversion::assert_healthy(resolutions)
    })
    .map_err(pyo3::exceptions::PyRuntimeError::new_err)
}

pub fn register(module: &pyo3::Bound<'_, pyo3::types::PyModule>) -> pyo3::PyResult<()> {
    use pyo3::types::PyModuleMethods;
    module.add_function(pyo3::wrap_pyfunction!(merge_py, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(augment_py, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(members_py, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(analyze_py, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(validate_compiled_py, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(additions_py, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(addition_sources_py, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(requires_addition_py, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(patch_source_py, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(install_py, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(validate_merged_py, module)?)?;
    module.add_function(pyo3::wrap_pyfunction!(healthy_py, module)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_preserves_named_state_and_remote_event_replacement() {
        let skeleton = "Scriptname Demo\nAuto State Old\nFunction ::remote_Actor_OnHit()\nEndFunction\nEndState\nEvent OnInit()\nGoToState(\"Old\")\nEndEvent\n";
        let patch = "; @state-rename Old New\nState Old\nEvent Actor.OnHit(Actor sender)\nDebug.Trace(\"hit\")\nEndEvent\nEndState\n";
        let merged = merge(skeleton, patch).unwrap();
        assert!(merged.contains("Auto State New\nEvent Actor.OnHit"));
        assert!(merged.contains("GoToState(\"New\")"));
        assert!(!merged.contains("::remote_"));
    }

    #[test]
    fn patch_extends_directive_reparents_header_and_keeps_flags() {
        let skeleton = "Scriptname Demo Extends Actor hidden\nInt Property Count Auto\n";
        let patch = "; @extends ReferenceAlias\nEvent OnAliasInit()\nEndEvent\n";
        let merged = merge(skeleton, patch).unwrap();
        assert!(merged.starts_with("Scriptname Demo Extends ReferenceAlias hidden\n"));
        assert!(merged.contains("Int Property Count Auto"));
        assert_eq!(merge(&merged, patch).unwrap(), merged);

        let bare = merge("Scriptname Demo\n", "; @extends ReferenceAlias\n").unwrap();
        assert!(bare.starts_with("Scriptname Demo Extends ReferenceAlias\n"));

        assert!(merge(
            "Scriptname Demo Extends Actor\n",
            "; @extends ReferenceAlias\n; @extends Quest\n"
        )
        .is_err());
    }

    #[test]
    fn augment_retypes_caps_currency_properties_as_misc_object() {
        let drinks = "Scriptname TopicInfoBuyDrinks Extends TopicInfo\n\ncurrency Property PaymentType Auto mandatory\nInt Property Price = 10 Auto\n";
        let out = augment("TopicInfoBuyDrinks", drinks).unwrap();
        assert!(out.contains("\nMiscObject Property PaymentType Auto mandatory\n"));

        let exchange = "Scriptname Quests:_Default:TopicInfoExchangeItemsOrCurrency Extends TopicInfo default\ncurrency Property PlayerCurrency_Type Auto\ncurrency Property NPCCurrency_Type Auto\n";
        let out = augment("Quests:_Default:TopicInfoExchangeItemsOrCurrency", exchange).unwrap();
        assert!(out.contains("MiscObject Property PlayerCurrency_Type Auto\nMiscObject Property NPCCurrency_Type Auto"));

        let other = "Scriptname Other Extends Quest\ncurrency Property PaymentType Auto\n";
        assert_eq!(augment("Other", other).unwrap(), other);
    }

    #[test]
    fn patch_rejects_lossy_headers_and_duplicate_members() {
        assert!(merge(
            "Scriptname Demo\n",
            "Thing[][][][][][][][] Function A..B()\nEndFunction\n"
        )
        .is_err());
        assert!(merge(
            "Scriptname Demo\n",
            "Event OnInit()\nEndEvent\nEvent OnInit()\nEndEvent\n"
        )
        .is_err());
    }

    #[test]
    fn patch_golden_corpus_when_requested() {
        let Ok(path) = std::env::var("BACUP_SCRIPT_PATCH_GOLDENS") else {
            return;
        };
        let cases: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        for case in cases["merge"].as_array().unwrap() {
            let actual = merge(
                case["skeleton"].as_str().unwrap(),
                case["patch"].as_str().unwrap(),
            );
            if let Some(expected) = case["expected"].as_str() {
                assert_eq!(actual.unwrap(), expected, "{}", case["name"]);
            } else {
                assert_eq!(
                    actual.unwrap_err(),
                    case["error"].as_str().unwrap(),
                    "{}",
                    case["name"]
                );
            }
        }
        for case in cases["augment"].as_array().unwrap() {
            assert_eq!(
                augment(
                    case["name"].as_str().unwrap(),
                    case["skeleton"].as_str().unwrap()
                )
                .unwrap(),
                case["expected"].as_str().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
}
