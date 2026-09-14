use std::collections::BTreeMap;

use crate::ids::FormKey;
use crate::sym::StringInterner;
use crate::translator::pair_hooks::fnv_fo4::{PlacedActorAliasResolver, PlacedActorAliasTarget};

use super::quest_ir::{LegacyTargetIdentity, QuestLoweringError};

pub(crate) trait LegacyRawFormIdResolver {
    fn resolve_raw_formid(&self, raw: u32) -> Option<FormKey>;
}

impl<F> LegacyRawFormIdResolver for F
where
    F: Fn(u32) -> Option<FormKey>,
{
    fn resolve_raw_formid(&self, raw: u32) -> Option<FormKey> {
        self(raw)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StableQuestAlias {
    pub alias_id: i32,
    pub name: String,
    pub source_placed: FormKey,
    pub target_placed: FormKey,
    pub target_placed_text: String,
    pub target_base: FormKey,
}

pub(crate) fn resolve_legacy_target(
    identity: &LegacyTargetIdentity,
    raw_formids: &dyn LegacyRawFormIdResolver,
    placed_aliases: &PlacedActorAliasResolver,
    interner: &StringInterner,
) -> Result<PlacedActorAliasTarget, QuestLoweringError> {
    let source_placed = match identity {
        LegacyTargetIdentity::RawFormId(raw) => raw_formids.resolve_raw_formid(*raw),
        LegacyTargetIdentity::FormKeyText(text) => parse_form_key(text, interner),
    }
    .ok_or_else(|| QuestLoweringError::MissingRequiredTarget {
        objective: 0,
        reason: format!("cannot resolve source identity {identity:?}"),
    })?;
    placed_aliases.resolve_direct(source_placed).ok_or_else(|| {
        QuestLoweringError::MissingRequiredTarget {
            objective: 0,
            reason: format!(
                "placed {:06X} has no emitted ACHR alias target",
                source_placed.local
            ),
        }
    })
}

pub(crate) fn allocate_stable_aliases(
    targets: impl IntoIterator<Item = PlacedActorAliasTarget>,
    interner: &StringInterner,
) -> Vec<StableQuestAlias> {
    let mut unique = BTreeMap::new();
    for target in targets {
        let plugin = interner
            .resolve(target.source_placed.plugin)
            .unwrap_or("")
            .to_ascii_lowercase();
        unique
            .entry((plugin, target.source_placed.local))
            .or_insert(target);
    }
    unique
        .into_values()
        .enumerate()
        .map(|(index, target)| StableQuestAlias {
            alias_id: index as i32,
            name: stable_alias_name(target.source_placed, interner),
            source_placed: target.source_placed,
            target_placed: target.target_placed,
            target_placed_text: form_key_text(target.target_placed, interner),
            target_base: target.target_base,
        })
        .collect()
}

pub(crate) fn alias_id_for_source(
    aliases: &[StableQuestAlias],
    source_placed: FormKey,
) -> Option<i32> {
    aliases
        .iter()
        .find(|alias| alias.source_placed == source_placed)
        .map(|alias| alias.alias_id)
}

pub(crate) fn form_key_text(form_key: FormKey, interner: &StringInterner) -> String {
    let plugin = interner.resolve(form_key.plugin).unwrap_or("");
    format!("{:06X}:{plugin}", form_key.local)
}

fn parse_form_key(text: &str, interner: &StringInterner) -> Option<FormKey> {
    let (object_id, plugin) = text.split_once(':')?;
    Some(FormKey {
        local: u32::from_str_radix(object_id.trim(), 16).ok()? & 0x00FF_FFFF,
        plugin: interner.intern(plugin.trim()),
    })
}

fn stable_alias_name(source_placed: FormKey, interner: &StringInterner) -> String {
    let plugin = interner.resolve(source_placed.plugin).unwrap_or("Source");
    let plugin_stem = plugin.rsplit_once('.').map_or(plugin, |(stem, _)| stem);
    let plugin_stem = plugin_stem
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("LegacyRef_{plugin_stem}_{:06X}", source_placed.local)
}
