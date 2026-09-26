//! Material swaps (`MSWP`) as CK 1.11.137 applies them to a reference's 3D
//! before combining (`0x358780`, lookup `0x440F80`).

use crate::error::{PrevisError, Result};
use crate::plugin::{Plugin, RecordRef, read_f32, zstring};

struct Entry {
    original: String,
    replacement: String,
    color_index: Option<f32>,
}

pub struct MaterialSwap {
    entries: Vec<Entry>,
}

pub fn read_swap(plugin: &Plugin, record: &RecordRef) -> Result<MaterialSwap> {
    let mut entries: Vec<Entry> = Vec::new();
    for (signature, data) in plugin.subrecords_at(record)?.iter() {
        let current = || {
            PrevisError::invalid(format!("MSWP {:08X} has a substitution field before BNAM", record.form_id))
        };
        match &signature {
            b"EDID" | b"FNAM" => {}
            b"BNAM" => {
                let original = zstring(data);
                // A repeated original overwrites the earlier entry.
                entries.retain(|e| !e.original.eq_ignore_ascii_case(&original));
                entries.push(Entry {
                    original,
                    replacement: String::new(),
                    color_index: None,
                });
            }
            b"SNAM" => entries.last_mut().ok_or_else(current)?.replacement = zstring(data),
            b"CNAM" => {
                let value = read_f32(data, 0).ok_or_else(|| PrevisError::invalid("MSWP CNAM is truncated"))?;
                entries.last_mut().ok_or_else(current)?.color_index = (value < f32::MAX).then_some(value);
            }
            other => {
                return Err(PrevisError::unsupported(format!(
                    "MSWP {:08X} has unverified field {}",
                    record.form_id,
                    String::from_utf8_lossy(other)
                )));
            }
        }
    }
    Ok(MaterialSwap { entries })
}

/// The part of a shader name the swap map is keyed by.
fn swap_key(shader_name: &str) -> &str {
    let lower = shader_name.to_ascii_lowercase();
    for prefix in ["data\\materials\\", "materials\\"] {
        if let Some(index) = lower.find(prefix) {
            return &shader_name[index + prefix.len()..];
        }
    }
    shader_name
}

/// `?`, `*` and `[...]` matching on lowercased strings, as CK's fallback does.
fn glob_match(pattern: &[u8], text: &[u8]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some((b'*', rest)) => (0..=text.len()).any(|skip| glob_match(rest, &text[skip..])),
        Some((b'?', rest)) => !text.is_empty() && glob_match(rest, &text[1..]),
        Some((b'[', rest)) => {
            let Some(close) = rest.iter().position(|&c| c == b']') else {
                return text.first() == Some(&b'[') && glob_match(rest, &text[1..]);
            };
            let Some((&c, text_rest)) = text.split_first() else { return false };
            let class = &rest[..close];
            let mut matched = false;
            let mut index = 0;
            while index < class.len() {
                if index + 2 < class.len() && class[index + 1] == b'-' {
                    matched |= (class[index]..=class[index + 2]).contains(&c);
                    index += 3;
                } else {
                    matched |= class[index] == c;
                    index += 1;
                }
            }
            matched && glob_match(&rest[close + 1..], text_rest)
        }
        Some((&p, rest)) => text.first() == Some(&p) && glob_match(rest, &text[1..]),
    }
}

impl MaterialSwap {
    fn exact(&self, key: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.original.eq_ignore_ascii_case(key))
    }

    /// The replacement material as a data-relative `materials\` path, or
    /// `None` when the shape keeps its own material.
    pub fn replacement(&self, shader_name: &str) -> Result<Option<String>> {
        let key = swap_key(shader_name);
        let replacement = match self.exact(key) {
            Some(entry) => entry.replacement.as_str(),
            None => {
                // The fallback walks hash slots, whose order is not reproduced,
                // so it is only used when every match agrees.
                let lower = key.to_ascii_lowercase();
                let mut matches = self
                    .entries
                    .iter()
                    .filter(|e| glob_match(e.original.to_ascii_lowercase().as_bytes(), lower.as_bytes()))
                    .map(|e| e.replacement.as_str());
                let Some(first) = matches.next() else { return Ok(None) };
                if matches.any(|other| other != first) {
                    return Err(PrevisError::unsupported(format!(
                        "several material swap patterns match {shader_name}"
                    )));
                }
                first
            }
        };
        if replacement.is_empty() || replacement == key {
            return Ok(None);
        }
        if replacement.contains('*') {
            return Err(PrevisError::unsupported("material swap wildcard substitution is not yet supported"));
        }
        Ok(Some(format!("materials\\{replacement}")))
    }

    /// CNAM of the exact entry for the *original* material; it applies even
    /// when no replacement does.
    pub fn color_index(&self, shader_name: &str) -> Option<f32> {
        self.exact(swap_key(shader_name)).and_then(|e| e.color_index)
    }
}

/// A placed reference's swap (`XMSP`) over its model's own (`MODS`).
///
/// CK output shows the reference swap winning wherever it replaces a
/// material. Whether the model swap still applies to a material the
/// reference swap leaves alone, or to the reference swap's replacement, was
/// not observed, so those cases are refused.
#[derive(Clone, Copy, Default)]
pub struct SwapLayers<'a> {
    pub reference: Option<&'a MaterialSwap>,
    pub model: Option<&'a MaterialSwap>,
}

impl SwapLayers<'_> {
    pub fn replacement(&self, shader_name: &str) -> Result<Option<String>> {
        let reference = self.reference.map(|s| s.replacement(shader_name)).transpose()?.flatten();
        let model = self.model.map(|s| s.replacement(shader_name)).transpose()?.flatten();
        match (reference, model) {
            (Some(found), _) if self.model.map(|s| s.replacement(&found)).transpose()?.flatten().is_some() => {
                Err(PrevisError::unsupported(format!(
                    "a model material swap of the reference swap's replacement for {shader_name} is not verified"
                )))
            }
            (Some(found), _) => Ok(Some(found)),
            (None, Some(_)) if self.reference.is_some() => Err(PrevisError::unsupported(format!(
                "a model material swap under a reference swap that keeps {shader_name} is not verified"
            ))),
            (None, model) => Ok(model),
        }
    }

    pub fn color_index(&self, shader_name: &str) -> Result<Option<f32>> {
        let reference = self.reference.and_then(|s| s.color_index(shader_name));
        let model = self.model.and_then(|s| s.color_index(shader_name));
        match (reference, model) {
            (Some(found), _) => Ok(Some(found)),
            (None, Some(_)) if self.reference.is_some() => Err(PrevisError::unsupported(format!(
                "a model swap color index under a reference swap for {shader_name} is not verified"
            ))),
            (None, model) => Ok(model),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn swap(entries: &[(&str, &str, Option<f32>)]) -> MaterialSwap {
        MaterialSwap {
            entries: entries
                .iter()
                .map(|&(original, replacement, color_index)| Entry {
                    original: original.into(),
                    replacement: replacement.into(),
                    color_index,
                })
                .collect(),
        }
    }

    #[test]
    fn keys_strip_through_the_first_materials_directory() {
        assert_eq!(swap_key("Materials\\A\\b.BGSM"), "A\\b.BGSM");
        assert_eq!(swap_key("C:\\Build\\Data\\Materials\\A\\b.bgsm"), "A\\b.bgsm");
        assert_eq!(swap_key("A\\b.bgsm"), "A\\b.bgsm");
    }

    #[test]
    fn exact_entries_win_and_empty_replacements_keep_the_material() {
        let s = swap(&[("a\\x.bgsm", "a\\y.bgsm", Some(0.5)), ("a\\*.bgsm", "a\\z.bgsm", None), ("a\\e.bgsm", "", None)]);
        assert_eq!(s.replacement("Materials\\A\\X.BGSM").unwrap().as_deref(), Some("materials\\a\\y.bgsm"));
        assert_eq!(s.replacement("Materials\\A\\w.bgsm").unwrap().as_deref(), Some("materials\\a\\z.bgsm"));
        assert_eq!(s.replacement("Materials\\A\\e.bgsm").unwrap(), None);
        assert_eq!(s.replacement("Materials\\B\\x.bgsm").unwrap(), None);
        assert_eq!(s.color_index("Materials\\A\\X.BGSM"), Some(0.5));
    }

    #[test]
    fn reference_swap_wins_and_an_unshadowed_model_swap_is_refused() {
        let reference = swap(&[("a\\x.bgsm", "a\\r.bgsm", Some(0.25))]);
        let model = swap(&[("a\\x.bgsm", "a\\m.bgsm", None), ("a\\y.bgsm", "a\\n.bgsm", Some(0.5))]);
        let layers = SwapLayers { reference: Some(&reference), model: Some(&model) };
        assert_eq!(layers.replacement("Materials\\a\\x.bgsm").unwrap().as_deref(), Some("materials\\a\\r.bgsm"));
        assert_eq!(layers.color_index("Materials\\a\\x.bgsm").unwrap(), Some(0.25));
        assert_eq!(layers.replacement("Materials\\a\\z.bgsm").unwrap(), None);
        assert!(layers.replacement("Materials\\a\\y.bgsm").is_err());
        assert!(layers.color_index("Materials\\a\\y.bgsm").is_err());
        let chained = swap(&[("a\\r.bgsm", "a\\s.bgsm", None)]);
        let layers = SwapLayers { reference: Some(&reference), model: Some(&chained) };
        assert!(layers.replacement("Materials\\a\\x.bgsm").is_err());
        let model_only = SwapLayers { reference: None, model: Some(&model) };
        assert_eq!(model_only.replacement("Materials\\a\\y.bgsm").unwrap().as_deref(), Some("materials\\a\\n.bgsm"));
        assert_eq!(model_only.color_index("Materials\\a\\y.bgsm").unwrap(), Some(0.5));
    }

    #[test]
    fn conflicting_patterns_are_unsupported() {
        let s = swap(&[("a\\*", "a\\y.bgsm", None), ("a\\[wx].bgsm", "a\\z.bgsm", None)]);
        assert!(s.replacement("Materials\\a\\x.bgsm").is_err());
        assert_eq!(s.replacement("Materials\\a\\q.bgsm").unwrap().as_deref(), Some("materials\\a\\y.bgsm"));
    }
}
