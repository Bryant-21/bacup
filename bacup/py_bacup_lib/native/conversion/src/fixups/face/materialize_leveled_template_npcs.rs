//! Replace LVLN entries that point at active template actors with concrete NPC clones.
//!
//! FO76 composes humanoid spawns from one Traits face list plus independent
//! role templates for factions, inventory, AI, packages, and keywords. FO4
//! runs those records, but the CK rejects them inside LVLN records. Expand the
//! Traits choices, bake each template category into a clone, and repoint the
//! list entry. Vanilla-remapped actors recover their FO76 template slots first
//! so unrelated scripts inherited from the matching FO4 actor are not baked in.
//! Human clones record a FaceGen alias so the post-asset phase can duplicate
//! the selected face NIF under the clone's FormID.

use std::fs;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::fixups::creature::creature_predicate::npc_acbs_template_flags;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::materialized_npc_facegen::{
    FacegenAlias, FacegenAliasManifest, MaterializedNpcEntry, manifest_path,
};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::session::PluginSession;
use crate::sym::StringInterner;

const SLOT_COUNT: usize = 13;
const TRAITS_SLOT: usize = 0;
const SCRIPT_SLOT: usize = 9;
const MAX_TRAITS_VARIANTS: usize = 64;
const MAX_DIAGNOSTIC_DETAILS: usize = 256;
const SYNTH_PLUGIN: &str = "__synth_materialized_npc__";
const SYNTH_LIST_PLUGIN: &str = "__synth_materialized_npc_list__";
const ACBS_TEMPLATE_FLAGS_OFFSET: usize = 14;
const ACBS_TRAITS_FLAG_MASK: u32 = 0x0008_0005;
const ACBS_STATS_FLAG_MASK: u32 = 0x0004_0090;
const ACBS_MODEL_ANIMATION_FLAG_MASK: u32 = 0x0000_0100;
const ACBS_ATTACK_DATA_FLAG_MASK: u32 = 0x0000_0400;
const ACBS_CALC_FOR_EACH_TEMPLATE_FLAG: u32 = 0x0000_0200;

#[derive(Clone, Copy)]
struct SourceTemplateInfo {
    source_actor: FormKey,
    slots: [Option<FormKey>; SLOT_COUNT],
    has_direct_vmad: bool,
}

#[derive(Default)]
struct MaterializerStats {
    lists_scanned: usize,
    list_read_failed: usize,
    entries_seen: usize,
    invalid_lvlo_references: usize,
    retarget_failed: usize,
    actor_cache_hits: usize,
    actor_read_failed: usize,
    non_npc_entries: usize,
    non_template_actors: usize,
    source_template_lookup_misses: usize,
    source_record_read_failed: usize,
    source_record_not_npc: usize,
    source_template_unmapped_refs: usize,
    source_template_overrides_unavailable: usize,
    traits_unresolved: usize,
    variant_failures: usize,
    all_variants_failed: usize,
    actors_materialized: usize,
    template_depth_limit: usize,
    template_cycles: usize,
    template_record_read_failed: usize,
    template_invalid_lvlo: usize,
    template_unsupported_terminals: usize,
}

enum VariantFailure {
    SlotUnresolved { slot: usize, target: FormKey },
    SourceRecordUnreadable { slot: usize, source: FormKey },
}

fn diagnostic_priority(detail: &str) -> u8 {
    if detail.starts_with("skip:scripted_actor") {
        0
    } else if detail.starts_with("issue:") {
        1
    } else if detail.starts_with("skip:") {
        2
    } else if detail.starts_with("decision:source_template_override") {
        3
    } else {
        4
    }
}

pub struct MaterializeLeveledTemplateNpcsFixup;

impl Fixup for MaterializeLeveledTemplateNpcsFixup {
    fn name(&self) -> &'static str {
        "materialize_leveled_template_npcs"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::WholePluginSafe
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, config: &FixupConfig) -> bool {
        config.is_whole_plugin
            && session
                .source_slot_opt()
                .and_then(|slot| slot.parsed.game.as_deref())
                == Some("fo76")
            && session.target_slot().parsed.game.as_deref() == Some("fo4")
            && config
                .root_sig
                .is_none_or(|sig| matches!(sig.as_str(), "NPC_" | "LVLN"))
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let schema = session
            .schema()
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let lvln_sig = SigCode::from_str("LVLN")
            .map_err(|error| FixupError::SchemaError(error.to_string()))?;
        let mut lists = session
            .form_keys_of_sig(lvln_sig, mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        lists.sort_by_key(|form_key| {
            (
                mapper.interner.resolve(form_key.plugin).unwrap_or(""),
                form_key.local,
            )
        });

        let target_masters = session.target_masters().to_vec();
        let target_plugin = session.target_slot().parsed.plugin_name.clone();
        let source_schema = session.source_schema().ok();
        let source_masters = session
            .source_slot_opt()
            .map(|slot| slot.parsed.header.masters.clone())
            .unwrap_or_default();
        let source_plugin = session
            .source_slot_opt()
            .map(|slot| slot.parsed.plugin_name.clone())
            .unwrap_or_default();
        let mut materializer = NpcMaterializer::new(
            session,
            mapper,
            config,
            Arc::clone(&schema),
            source_schema,
            target_masters,
            target_plugin,
            source_masters,
            source_plugin,
        );
        if materializer.source_schema.is_none() {
            materializer.record_diagnostic("issue:source_schema_unavailable".to_string());
        }
        let mut changed_lists = Vec::new();
        for list in lists {
            materializer.stats.lists_scanned += 1;
            let Some(mut record) = materializer.read_record(list) else {
                materializer.unresolved += 1;
                materializer.stats.list_read_failed += 1;
                let list = materializer.form_key_label(list);
                materializer.record_diagnostic(format!("issue:list_read_failed:list={list}"));
                continue;
            };
            if materializer.rewrite_list_entries(&mut record) {
                changed_lists.push(record);
            }
        }

        let concrete_npcs_added = materializer.clones.len();
        let variant_lists_added = materializer.variant_lists.len();
        let records_added = concrete_npcs_added + variant_lists_added;
        let lists_changed = changed_lists.len();
        let aliases = materializer.aliases.clone();
        let materializations = materializer.materializations.clone();
        let scripted_skipped = materializer.scripted_skipped;
        let mut diagnostic_details = materializer
            .diagnostic_details
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        diagnostic_details.sort_by(|left, right| {
            diagnostic_priority(left)
                .cmp(&diagnostic_priority(right))
                .then_with(|| left.cmp(right))
        });
        let diagnostic_detail_count = diagnostic_details.len();
        let diagnostic_details_suppressed = materializer.diagnostic_details_suppressed;
        let source_template_overrides = materializer.source_template_overrides;
        let unresolved = materializer.unresolved;
        let variants_collapsed = materializer.variants_collapsed;
        let variants_truncated = materializer.variants_truncated;
        let stats = &materializer.stats;
        let lists_scanned = stats.lists_scanned;
        let list_read_failed = stats.list_read_failed;
        let entries_seen = stats.entries_seen;
        let invalid_lvlo_references = stats.invalid_lvlo_references;
        let retarget_failed = stats.retarget_failed;
        let actor_cache_hits = stats.actor_cache_hits;
        let actor_read_failed = stats.actor_read_failed;
        let non_npc_entries = stats.non_npc_entries;
        let non_template_actors = stats.non_template_actors;
        let source_template_lookup_misses = stats.source_template_lookup_misses;
        let source_record_read_failed = stats.source_record_read_failed;
        let source_record_not_npc = stats.source_record_not_npc;
        let source_template_unmapped_refs = stats.source_template_unmapped_refs;
        let source_template_overrides_unavailable = stats.source_template_overrides_unavailable;
        let traits_unresolved = stats.traits_unresolved;
        let variant_failures = stats.variant_failures;
        let all_variants_failed = stats.all_variants_failed;
        let actors_materialized = stats.actors_materialized;
        let template_depth_limit = stats.template_depth_limit;
        let template_cycles = stats.template_cycles;
        let template_record_read_failed = stats.template_record_read_failed;
        let template_invalid_lvlo = stats.template_invalid_lvlo;
        let template_unsupported_terminals = stats.template_unsupported_terminals;

        if !materializer.clones.is_empty() || !materializer.variant_lists.is_empty() {
            let mut additions = std::mem::take(&mut materializer.clones);
            additions.extend(std::mem::take(&mut materializer.variant_lists));
            materializer
                .session
                .add_records(additions, schema.as_ref(), materializer.mapper.interner)
                .map_err(|error| FixupError::HandleError(error.to_string()))?;
        }
        if !changed_lists.is_empty() {
            materializer
                .session
                .replace_records_contents(
                    changed_lists,
                    schema.as_ref(),
                    materializer.mapper.interner,
                )
                .map_err(|error| FixupError::HandleError(error.to_string()))?;
        }
        write_manifest(config, aliases, materializations)?;

        let mut report = FixupReport::empty();
        report.records_added = records_added.try_into().unwrap_or(u32::MAX);
        report.records_changed = lists_changed.try_into().unwrap_or(u32::MAX);
        for detail in diagnostic_details {
            let informational = detail.starts_with("decision:");
            let detail = materializer
                .mapper
                .interner
                .intern(&format!("materialize_leveled_template_npcs:{detail}"));
            if informational {
                report.diagnostics.push(detail);
            } else {
                report.warnings.push(detail);
            }
        }
        if lists_scanned != 0 || !report.warnings.is_empty() || !report.diagnostics.is_empty() {
            report.message = Some(materializer.mapper.interner.intern(&format!(
                "lists_changed={lists_changed} concrete_npcs_added={concrete_npcs_added} \
                 variant_lists_added={variant_lists_added} \
                 facegen_aliases={} scripted_skipped={scripted_skipped} \
                 source_template_overrides={source_template_overrides} \
                 unresolved={unresolved} variants_collapsed={variants_collapsed} \
                 variants_truncated={variants_truncated} \
                 lists_scanned={lists_scanned} list_read_failed={list_read_failed} \
                 entries_seen={entries_seen} invalid_lvlo_references={invalid_lvlo_references} \
                 retarget_failed={retarget_failed} actor_cache_hits={actor_cache_hits} \
                 actor_read_failed={actor_read_failed} non_npc_entries={non_npc_entries} \
                 non_template_actors={non_template_actors} \
                 source_template_lookup_misses={source_template_lookup_misses} \
                 source_record_read_failed={source_record_read_failed} \
                 source_record_not_npc={source_record_not_npc} \
                 source_template_unmapped_refs={source_template_unmapped_refs} \
                 source_template_overrides_unavailable={source_template_overrides_unavailable} \
                 traits_unresolved={traits_unresolved} variant_failures={variant_failures} \
                 all_variants_failed={all_variants_failed} \
                 actors_materialized={actors_materialized} \
                 template_depth_limit={template_depth_limit} \
                 template_cycles={template_cycles} \
                 template_record_read_failed={template_record_read_failed} \
                 template_invalid_lvlo={template_invalid_lvlo} \
                 template_unsupported_terminals={template_unsupported_terminals} \
                 diagnostic_details={diagnostic_detail_count} \
                 diagnostic_details_suppressed={diagnostic_details_suppressed}",
                materializer.aliases.len(),
            )));
        }
        Ok(report)
    }
}

fn write_manifest(
    config: &FixupConfig,
    mut aliases: Vec<FacegenAlias>,
    mut materializations: Vec<MaterializedNpcEntry>,
) -> Result<(), FixupError> {
    let Some(mod_path) = config.mod_path.as_deref() else {
        return Ok(());
    };
    aliases.sort_by(|left, right| {
        (
            left.target_plugin.as_str(),
            left.target_local,
            left.source_plugin.as_str(),
            left.source_local,
        )
            .cmp(&(
                right.target_plugin.as_str(),
                right.target_local,
                right.source_plugin.as_str(),
                right.source_local,
            ))
    });
    aliases.dedup();
    materializations.sort_by(|left, right| {
        (
            left.source_actor_plugin.as_str(),
            left.source_actor_local,
            left.traits_source_plugin.as_str(),
            left.traits_source_local,
            left.clone_local,
        )
            .cmp(&(
                right.source_actor_plugin.as_str(),
                right.source_actor_local,
                right.traits_source_plugin.as_str(),
                right.traits_source_local,
                right.clone_local,
            ))
    });
    let path = manifest_path(mod_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| FixupError::Other(format!("create facegen manifest dir: {error}")))?;
    }
    let payload = serde_json::to_vec_pretty(&FacegenAliasManifest {
        aliases,
        materializations,
    })
    .map_err(|error| FixupError::Other(format!("encode facegen manifest: {error}")))?;
    fs::write(&path, payload)
        .map_err(|error| FixupError::Other(format!("write {}: {error}", path.display())))
}

struct NpcMaterializer<'a, 'store, 'interner> {
    session: &'a mut PluginSession<'store>,
    mapper: &'a mut FormKeyMapper<'interner>,
    config: &'a FixupConfig,
    schema: Arc<AuthoringSchema>,
    source_schema: Option<Arc<AuthoringSchema>>,
    target_masters: Vec<String>,
    target_plugin: String,
    source_masters: Vec<String>,
    source_plugin: String,
    record_cache: FxHashMap<FormKey, Option<Record>>,
    source_template_cache: FxHashMap<FormKey, Option<SourceTemplateInfo>>,
    variant_cache: FxHashMap<FormKey, FormKey>,
    clones: Vec<Record>,
    variant_lists: Vec<Record>,
    aliases: Vec<FacegenAlias>,
    materializations: Vec<MaterializedNpcEntry>,
    next_synth_local: u32,
    next_synth_list_local: u32,
    scripted_skipped: usize,
    diagnostic_details: FxHashSet<String>,
    diagnostic_details_suppressed: usize,
    stats: MaterializerStats,
    source_template_overrides: usize,
    unresolved: usize,
    variants_collapsed: usize,
    variants_truncated: usize,
}

impl<'a, 'store, 'interner> NpcMaterializer<'a, 'store, 'interner> {
    fn new(
        session: &'a mut PluginSession<'store>,
        mapper: &'a mut FormKeyMapper<'interner>,
        config: &'a FixupConfig,
        schema: Arc<AuthoringSchema>,
        source_schema: Option<Arc<AuthoringSchema>>,
        target_masters: Vec<String>,
        target_plugin: String,
        source_masters: Vec<String>,
        source_plugin: String,
    ) -> Self {
        Self {
            session,
            mapper,
            config,
            schema,
            source_schema,
            target_masters,
            target_plugin,
            source_masters,
            source_plugin,
            record_cache: FxHashMap::default(),
            source_template_cache: FxHashMap::default(),
            variant_cache: FxHashMap::default(),
            clones: Vec::new(),
            variant_lists: Vec::new(),
            aliases: Vec::new(),
            materializations: Vec::new(),
            next_synth_local: 1,
            next_synth_list_local: 1,
            scripted_skipped: 0,
            diagnostic_details: FxHashSet::default(),
            diagnostic_details_suppressed: 0,
            stats: MaterializerStats::default(),
            source_template_overrides: 0,
            unresolved: 0,
            variants_collapsed: 0,
            variants_truncated: 0,
        }
    }

    fn read_record(&mut self, form_key: FormKey) -> Option<Record> {
        if let Some(cached) = self.record_cache.get(&form_key) {
            return cached.clone();
        }
        let result = read_target_or_master_record(
            self.session,
            self.schema.as_ref(),
            self.mapper.interner,
            self.mapper.interner.intern(&self.target_plugin),
            &self.target_masters,
            &self.config.target_master_handle_ids,
            form_key,
        );
        self.record_cache.insert(form_key, result.clone());
        result
    }

    fn form_key_label(&self, form_key: FormKey) -> String {
        let plugin = self
            .mapper
            .interner
            .resolve(form_key.plugin)
            .unwrap_or("<unknown>");
        format!("{plugin}:{:06X}", form_key.local & 0x00FF_FFFF)
    }

    fn record_label(&self, record: &Record) -> String {
        let form_key = self.form_key_label(record.form_key);
        let editor_id = record
            .eid
            .and_then(|eid| self.mapper.interner.resolve(eid))
            .unwrap_or("<no_edid>");
        format!("{form_key}:{editor_id}")
    }

    fn record_diagnostic(&mut self, detail: String) {
        if self.diagnostic_details.contains(&detail) {
            return;
        }
        if self.diagnostic_details.len() < MAX_DIAGNOSTIC_DETAILS {
            self.diagnostic_details.insert(detail);
            return;
        }
        let incoming_priority = diagnostic_priority(&detail);
        let replace = self
            .diagnostic_details
            .iter()
            .filter(|existing| diagnostic_priority(existing) > incoming_priority)
            .max_by_key(|existing| diagnostic_priority(existing))
            .cloned();
        if let Some(replace) = replace {
            self.diagnostic_details.remove(&replace);
            self.diagnostic_details.insert(detail);
        }
        self.diagnostic_details_suppressed += 1;
    }

    fn rewrite_list_entries(&mut self, record: &mut Record) -> bool {
        let mut changed = false;
        let list = self.record_label(record);
        let mut output: SmallVec<[FieldEntry; 8]> = SmallVec::with_capacity(record.fields.len());
        let fields = std::mem::take(&mut record.fields);
        let mut index = 0;
        let mut entry_index = 0;
        while index < fields.len() {
            if fields[index].sig.as_str() != "LVLO" {
                output.push(fields[index].clone());
                index += 1;
                continue;
            }
            let block_start = index;
            index += 1;
            while index < fields.len() && is_lvlo_tail(fields[index].sig.as_str()) {
                index += 1;
            }
            let block = &fields[block_start..index];
            self.stats.entries_seen += 1;
            let Some(entry) = lvlo_reference(
                &block[0].value,
                &self.target_masters,
                &self.target_plugin,
                self.mapper.interner,
            ) else {
                self.stats.invalid_lvlo_references += 1;
                self.record_diagnostic(format!(
                    "issue:invalid_lvlo_reference:list={list}:entry={entry_index}"
                ));
                output.extend(block.iter().cloned());
                entry_index += 1;
                continue;
            };
            let Some(replacement) = self.materialize_actor(entry) else {
                output.extend(block.iter().cloned());
                entry_index += 1;
                continue;
            };
            let mut rewritten = block.to_vec();
            if set_lvlo_reference(
                &mut rewritten[0].value,
                replacement,
                &self.target_masters,
                &self.target_plugin,
                self.mapper.interner,
            ) {
                output.extend(rewritten);
                changed = true;
            } else {
                self.stats.retarget_failed += 1;
                let actor = self.form_key_label(entry);
                let replacement = self.form_key_label(replacement);
                self.record_diagnostic(format!(
                    "issue:retarget_failed:list={list}:entry={entry_index}:actor={actor}:replacement={replacement}"
                ));
                output.extend(block.iter().cloned());
            }
            entry_index += 1;
        }
        record.fields = output;
        changed
    }

    fn materialize_actor(&mut self, actor: FormKey) -> Option<FormKey> {
        if let Some(cached) = self.variant_cache.get(&actor) {
            self.stats.actor_cache_hits += 1;
            return Some(*cached);
        }
        let Some(base) = self.read_record(actor) else {
            self.stats.actor_read_failed += 1;
            let actor = self.form_key_label(actor);
            self.record_diagnostic(format!("issue:actor_read_failed:actor={actor}"));
            return None;
        };
        if base.sig.as_str() != "NPC_" {
            self.stats.non_npc_entries += 1;
            return None;
        }
        let mut slots = template_slots(
            &base,
            &self.target_masters,
            &self.target_plugin,
            self.mapper.interner,
        );
        let target_plugin = self.mapper.interner.intern(&self.target_plugin);
        let source_template = (actor.plugin != target_plugin)
            .then(|| self.source_template_info(actor))
            .flatten();
        if let Some(source_template) = source_template
            && source_template.slots.iter().any(Option::is_some)
        {
            slots = source_template.slots;
            self.source_template_overrides += 1;
            let actor = self.record_label(&base);
            let source_actor = self.form_key_label(source_template.source_actor);
            let active_slots = slots.iter().filter(|slot| slot.is_some()).count();
            self.record_diagnostic(format!(
                "decision:source_template_override:actor={actor}:source={source_actor}:active_slots={active_slots}"
            ));
        } else if actor.plugin != target_plugin && slots.iter().any(Option::is_some) {
            self.stats.source_template_overrides_unavailable += 1;
            let actor = self.record_label(&base);
            let reason = if source_template.is_some() {
                "source_has_no_active_slots"
            } else {
                "source_record_unavailable"
            };
            self.record_diagnostic(format!(
                "skip:source_template_override_unavailable:actor={actor}:reason={reason}:target_slots_preserved"
            ));
        }
        if slots.iter().all(Option::is_none) {
            self.stats.non_template_actors += 1;
            return None;
        }
        let scripted_reason = if source_template.is_some_and(|source| source.has_direct_vmad) {
            Some("source_direct_vmad")
        } else if base.fields.iter().any(|field| field.sig.as_str() == "VMAD") {
            Some("target_direct_vmad")
        } else if slots[SCRIPT_SLOT].is_some_and(|target| self.slot_source_has_vmad(target)) {
            Some("template_vmad")
        } else {
            None
        };
        if let Some(reason) = scripted_reason {
            self.record_scripted_skip(actor, &base, reason);
            return None;
        }

        let mut traits_sources = if let Some(target) = slots[TRAITS_SLOT] {
            self.resolve_slot_terminals(target, TRAITS_SLOT, &mut FxHashSet::default(), 0)
        } else {
            vec![actor]
        };
        if traits_sources.is_empty() {
            self.unresolved += 1;
            self.stats.traits_unresolved += 1;
            let actor = self.record_label(&base);
            let traits_target = slots[TRAITS_SLOT]
                .map(|target| self.form_key_label(target))
                .unwrap_or_else(|| "<none>".to_string());
            self.record_diagnostic(format!(
                "issue:traits_unresolved:actor={actor}:target={traits_target}"
            ));
            return None;
        }
        // Repeated LVLO leaves are weights, so duplicate choices must survive.
        if let Some(base_female) = npc_is_female(&base, self.mapper.interner) {
            let matching = traits_sources
                .iter()
                .filter_map(|candidate| {
                    self.read_record(*candidate)
                        .filter(|record| {
                            npc_is_female(record, self.mapper.interner) == Some(base_female)
                        })
                        .map(|_| *candidate)
                })
                .collect::<Vec<_>>();
            if !matching.is_empty() {
                let removed = traits_sources.len() - matching.len();
                self.variants_collapsed += removed;
                if removed != 0 {
                    let actor = self.record_label(&base);
                    self.record_diagnostic(format!(
                        "decision:sex_filter:actor={actor}:removed={removed}:kept={}",
                        matching.len()
                    ));
                }
                traits_sources = matching;
            }
        }
        if traits_sources.len() > MAX_TRAITS_VARIANTS {
            let removed = traits_sources.len() - MAX_TRAITS_VARIANTS;
            self.variants_truncated += removed;
            let actor = self.record_label(&base);
            self.record_diagnostic(format!(
                "decision:variant_truncate:actor={actor}:removed={removed}:kept={MAX_TRAITS_VARIANTS}"
            ));
            traits_sources.truncate(MAX_TRAITS_VARIANTS);
        }

        let mut variants = Vec::with_capacity(traits_sources.len());
        for traits_source in traits_sources {
            let mut clone = match self.materialize_variant(&base, &slots, traits_source) {
                Ok(clone) => clone,
                Err(failure) => {
                    self.unresolved += 1;
                    self.stats.variant_failures += 1;
                    let actor = self.record_label(&base);
                    let traits_source = self.form_key_label(traits_source);
                    let detail = match failure {
                        VariantFailure::SlotUnresolved { slot, target } => {
                            let target = self.form_key_label(target);
                            format!(
                                "issue:variant_slot_unresolved:actor={actor}:traits={traits_source}:slot={}:target={target}",
                                template_slot_name(slot)
                            )
                        }
                        VariantFailure::SourceRecordUnreadable { slot, source } => {
                            let source = self.form_key_label(source);
                            format!(
                                "issue:variant_source_read_failed:actor={actor}:traits={traits_source}:slot={}:source={source}",
                                template_slot_name(slot)
                            )
                        }
                    };
                    self.record_diagnostic(detail);
                    continue;
                }
            };
            let synth_source = FormKey {
                local: self.next_synth_local,
                plugin: self.mapper.interner.intern(SYNTH_PLUGIN),
            };
            self.next_synth_local = self.next_synth_local.saturating_add(1);
            let clone_fk = self.mapper.allocate_or_resolve(
                synth_source,
                None,
                SigCode::from_str("NPC_").expect("NPC_ signature"),
            );
            clone.form_key = clone_fk;
            let base_eid = base
                .eid
                .and_then(|symbol| self.mapper.interner.resolve(symbol))
                .unwrap_or("TemplateActor");
            set_editor_id(
                &mut clone,
                &format!("{base_eid}_MAT_{:06X}", clone_fk.local & 0x00FF_FFFF),
                self.mapper.interner,
            );

            let face_source = self
                .read_record(traits_source)
                .filter(has_human_face_data)
                .map(|record| record.form_key)
                .or_else(|| has_human_face_data(&base).then_some(base.form_key));
            if let Some(face_source) = face_source {
                let source_plugin = self
                    .mapper
                    .interner
                    .resolve(face_source.plugin)
                    .unwrap_or("")
                    .to_string();
                self.aliases.push(FacegenAlias {
                    source_plugin,
                    source_local: face_source.local & 0x00FF_FFFF,
                    target_plugin: self.target_plugin.clone(),
                    target_local: clone_fk.local & 0x00FF_FFFF,
                });
            }
            self.materializations.push(MaterializedNpcEntry {
                source_actor_plugin: self
                    .mapper
                    .interner
                    .resolve(actor.plugin)
                    .unwrap_or("")
                    .to_string(),
                source_actor_local: actor.local & 0x00FF_FFFF,
                traits_source_plugin: self
                    .mapper
                    .interner
                    .resolve(traits_source.plugin)
                    .unwrap_or("")
                    .to_string(),
                traits_source_local: traits_source.local & 0x00FF_FFFF,
                clone_local: clone_fk.local & 0x00FF_FFFF,
            });
            variants.push(clone_fk);
            self.clones.push(clone);
        }
        if variants.is_empty() {
            self.stats.all_variants_failed += 1;
            let actor = self.record_label(&base);
            self.record_diagnostic(format!("skip:all_variants_failed:actor={actor}"));
            return None;
        }
        self.stats.actors_materialized += 1;
        let list_fk = self.build_variant_list(&base, variants);
        self.variant_cache.insert(actor, list_fk);
        Some(list_fk)
    }

    fn source_template_info(&mut self, target_actor: FormKey) -> Option<SourceTemplateInfo> {
        if let Some(cached) = self.source_template_cache.get(&target_actor) {
            return *cached;
        }

        let source_plugin = self.mapper.interner.intern(&self.source_plugin);
        let same_local = FormKey {
            local: target_actor.local,
            plugin: source_plugin,
        };
        let mut result = (self.mapper.lookup(same_local) == Some(target_actor))
            .then(|| self.source_template_info_for_actor(same_local))
            .flatten();
        if result.is_none() {
            let candidates = self
                .mapper
                .source_to_target_iter()
                .filter_map(|(source, target)| {
                    (target == target_actor && source != same_local).then_some(source)
                })
                .collect::<Vec<_>>();
            result = candidates
                .into_iter()
                .find_map(|source| self.source_template_info_for_actor(source));
        }
        if result.is_none() {
            self.stats.source_template_lookup_misses += 1;
        }
        self.source_template_cache.insert(target_actor, result);
        result
    }

    fn source_template_info_for_actor(
        &mut self,
        source_actor: FormKey,
    ) -> Option<SourceTemplateInfo> {
        let source_schema = self.source_schema.clone()?;
        let source = match self.session.source_record_decoded(
            &source_actor,
            source_schema.as_ref(),
            self.mapper.interner,
        ) {
            Ok(source) => source,
            Err(_) => {
                self.stats.source_record_read_failed += 1;
                let source_actor = self.form_key_label(source_actor);
                self.record_diagnostic(format!(
                    "issue:source_record_read_failed:actor={source_actor}"
                ));
                return None;
            }
        };
        if source.sig.as_str() != "NPC_" {
            self.stats.source_record_not_npc += 1;
            let source_actor = self.form_key_label(source_actor);
            self.record_diagnostic(format!(
                "issue:source_record_not_npc:actor={source_actor}:signature={}",
                source.sig.as_str()
            ));
            return None;
        }
        let has_direct_vmad = source
            .fields
            .iter()
            .any(|field| field.sig.as_str() == "VMAD");
        let mut slots = template_slots(
            &source,
            &self.source_masters,
            &self.source_plugin,
            self.mapper.interner,
        );
        for (slot_index, slot) in slots.iter_mut().enumerate() {
            let Some(source) = *slot else {
                continue;
            };
            if let Some(target) = self.mapper.lookup(source) {
                *slot = Some(target);
            } else {
                *slot = None;
                self.stats.source_template_unmapped_refs += 1;
                let source_actor = self.form_key_label(source_actor);
                let source = self.form_key_label(source);
                self.record_diagnostic(format!(
                    "issue:source_template_ref_unmapped:actor={source_actor}:slot={}:source={source}",
                    template_slot_name(slot_index)
                ));
            }
        }
        let default_template = source
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "TPLT")
            .and_then(|field| {
                form_key_value(
                    &field.value,
                    &self.source_masters,
                    &self.source_plugin,
                    self.mapper.interner,
                )
            })
            .and_then(|source| {
                let target = self.mapper.lookup(source);
                if target.is_none() {
                    self.stats.source_template_unmapped_refs += 1;
                    let source_actor = self.form_key_label(source_actor);
                    let source = self.form_key_label(source);
                    self.record_diagnostic(format!(
                        "issue:source_default_template_unmapped:actor={source_actor}:source={source}"
                    ));
                }
                target
            });
        if let Some(default_template) = default_template
            && let Some(flags) = npc_acbs_template_flags(&source)
        {
            for (index, slot) in slots.iter_mut().enumerate() {
                if flags & (1_u16 << index) != 0 && slot.is_none() {
                    *slot = Some(default_template);
                }
            }
        }
        Some(SourceTemplateInfo {
            source_actor,
            slots,
            has_direct_vmad,
        })
    }

    fn record_scripted_skip(&mut self, actor: FormKey, base: &Record, reason: &'static str) {
        self.scripted_skipped += 1;
        let plugin = self
            .mapper
            .interner
            .resolve(actor.plugin)
            .unwrap_or("<unknown>");
        let editor_id = base
            .eid
            .and_then(|eid| self.mapper.interner.resolve(eid))
            .unwrap_or("<no_edid>");
        self.record_diagnostic(format!(
            "skip:scripted_actor:actor={plugin}:{:06X}:{editor_id}:reason={reason}",
            actor.local & 0x00FF_FFFF
        ));
    }

    fn build_variant_list(&mut self, base: &Record, variants: Vec<FormKey>) -> FormKey {
        let source = FormKey {
            local: self.next_synth_list_local,
            plugin: self.mapper.interner.intern(SYNTH_LIST_PLUGIN),
        };
        self.next_synth_list_local = self.next_synth_list_local.saturating_add(1);
        let list_fk = self.mapper.allocate_or_resolve(
            source,
            None,
            SigCode::from_str("LVLN").expect("LVLN signature"),
        );
        let mut list = Record::new(SigCode::from_str("LVLN").expect("LVLN signature"), list_fk);
        let base_eid = base
            .eid
            .and_then(|symbol| self.mapper.interner.resolve(symbol))
            .unwrap_or("TemplateActor");
        set_editor_id(
            &mut list,
            &format!("{base_eid}_MAT_LIST_{:06X}", list_fk.local & 0x00FF_FFFF),
            self.mapper.interner,
        );
        list.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("LVLD").expect("LVLD signature"),
            value: FieldValue::Uint(0),
        });
        list.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("LLCT").expect("LLCT signature"),
            value: FieldValue::Uint(variants.len() as u64),
        });
        let level = self.mapper.interner.intern("Level");
        let unknown_u8_1 = self.mapper.interner.intern("unknown_u8_1");
        let unknown_u8_2 = self.mapper.interner.intern("unknown_u8_2");
        let npc = self.mapper.interner.intern("NPC");
        let count = self.mapper.interner.intern("Count");
        let chance_none = self.mapper.interner.intern("chance_none");
        let unknown_u8_6 = self.mapper.interner.intern("unknown_u8_6");
        for variant in variants {
            list.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("LVLO").expect("LVLO signature"),
                value: FieldValue::Struct(vec![
                    (
                        level,
                        FieldValue::Bytes(1u16.to_le_bytes().as_slice().into()),
                    ),
                    (unknown_u8_1, FieldValue::Bytes(vec![0].into())),
                    (unknown_u8_2, FieldValue::Bytes(vec![0].into())),
                    (npc, FieldValue::FormKey(variant)),
                    (
                        count,
                        FieldValue::Bytes(1u16.to_le_bytes().as_slice().into()),
                    ),
                    (chance_none, FieldValue::Bytes(vec![0].into())),
                    (unknown_u8_6, FieldValue::Bytes(vec![0].into())),
                ]),
            });
        }
        self.variant_lists.push(list);
        list_fk
    }

    fn materialize_variant(
        &mut self,
        base: &Record,
        slots: &[Option<FormKey>; SLOT_COUNT],
        traits_source: FormKey,
    ) -> Result<Record, VariantFailure> {
        let mut output = base.clone();
        for slot in 0..SLOT_COUNT {
            let source_fk = if slot == TRAITS_SLOT {
                slots[slot].map(|_| traits_source)
            } else {
                slots[slot].and_then(|target| {
                    let terminals =
                        self.resolve_slot_terminals(target, slot, &mut FxHashSet::default(), 0);
                    terminals
                        .contains(&traits_source)
                        .then_some(traits_source)
                        .or_else(|| terminals.into_iter().next())
                })
            };
            let Some(source_fk) = source_fk else {
                if let Some(target) = slots[slot] {
                    return Err(VariantFailure::SlotUnresolved { slot, target });
                }
                continue;
            };
            let Some(source) = self.read_record(source_fk) else {
                return Err(VariantFailure::SourceRecordUnreadable {
                    slot,
                    source: source_fk,
                });
            };
            overlay_template_category(&mut output, &source, slot, self.mapper.interner);
        }
        output
            .fields
            .retain(|field| !matches!(field.sig.as_str(), "TPTA" | "TPLT"));
        clear_acbs_template_flags(&mut output, self.mapper.interner);
        Ok(output)
    }

    fn resolve_slot_terminals(
        &mut self,
        current: FormKey,
        slot: usize,
        visited: &mut FxHashSet<FormKey>,
        depth: u8,
    ) -> Vec<FormKey> {
        if depth > 12 {
            self.stats.template_depth_limit += 1;
            let current = self.form_key_label(current);
            self.record_diagnostic(format!(
                "issue:template_depth_limit:slot={}:current={current}:depth={depth}",
                template_slot_name(slot)
            ));
            return Vec::new();
        }
        if !visited.insert(current) {
            self.stats.template_cycles += 1;
            let current = self.form_key_label(current);
            self.record_diagnostic(format!(
                "issue:template_cycle:slot={}:current={current}:depth={depth}",
                template_slot_name(slot)
            ));
            return Vec::new();
        }
        let Some(record) = self.read_record(current) else {
            self.stats.template_record_read_failed += 1;
            let current = self.form_key_label(current);
            self.record_diagnostic(format!(
                "issue:template_record_read_failed:slot={}:current={current}",
                template_slot_name(slot)
            ));
            return Vec::new();
        };
        match record.sig.as_str() {
            "LVLN" => {
                let mut terminals = Vec::new();
                for field in &record.fields {
                    if field.sig.as_str() != "LVLO" {
                        continue;
                    }
                    let Some(entry) = lvlo_reference(
                        &field.value,
                        &self.target_masters,
                        &self.target_plugin,
                        self.mapper.interner,
                    ) else {
                        self.stats.template_invalid_lvlo += 1;
                        let current = self.record_label(&record);
                        self.record_diagnostic(format!(
                            "issue:template_invalid_lvlo:list={current}:slot={}",
                            template_slot_name(slot)
                        ));
                        continue;
                    };
                    let mut branch_visited = visited.clone();
                    terminals.extend(self.resolve_slot_terminals(
                        entry,
                        slot,
                        &mut branch_visited,
                        depth + 1,
                    ));
                }
                terminals
            }
            "NPC_" => {
                let slots = template_slots(
                    &record,
                    &self.target_masters,
                    &self.target_plugin,
                    self.mapper.interner,
                );
                if let Some(next) = slots[slot] {
                    self.resolve_slot_terminals(next, slot, visited, depth + 1)
                } else {
                    vec![record.form_key]
                }
            }
            _ => {
                self.stats.template_unsupported_terminals += 1;
                let current = self.record_label(&record);
                self.record_diagnostic(format!(
                    "issue:template_unsupported_terminal:record={current}:signature={}:slot={}",
                    record.sig.as_str(),
                    template_slot_name(slot)
                ));
                Vec::new()
            }
        }
    }

    fn slot_source_has_vmad(&mut self, target: FormKey) -> bool {
        self.resolve_slot_terminals(target, SCRIPT_SLOT, &mut FxHashSet::default(), 0)
            .into_iter()
            .any(|source| {
                self.read_record(source).is_some_and(|record| {
                    record
                        .fields
                        .iter()
                        .any(|field| field.sig.as_str() == "VMAD")
                })
            })
    }
}

fn target_master_handle_for_fk(
    form_key: FormKey,
    target_masters: &[String],
    target_master_handle_ids: &[u64],
    interner: &StringInterner,
) -> Option<u64> {
    let plugin = interner.resolve(form_key.plugin)?;
    let index = target_masters
        .iter()
        .position(|master| master.eq_ignore_ascii_case(plugin))?;
    target_master_handle_ids.get(index).copied()
}

pub(crate) fn read_target_or_master_record(
    session: &mut PluginSession<'_>,
    schema: &AuthoringSchema,
    interner: &StringInterner,
    target_plugin: crate::sym::Sym,
    target_masters: &[String],
    target_master_handle_ids: &[u64],
    form_key: FormKey,
) -> Option<Record> {
    if form_key.plugin == target_plugin {
        return session.record_decoded(&form_key, schema, interner).ok();
    }
    let handle =
        target_master_handle_for_fk(form_key, target_masters, target_master_handle_ids, interner)?;
    session
        .record_decoded_in_handle(handle, &form_key, schema, interner)
        .ok()
}

fn template_slots(
    record: &Record,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> [Option<FormKey>; SLOT_COUNT] {
    let mut slots = [None; SLOT_COUNT];
    let Some(value) = record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "TPTA")
        .map(|field| &field.value)
    else {
        return slots;
    };
    match value {
        FieldValue::Bytes(bytes) => {
            for (index, chunk) in bytes.chunks_exact(4).take(SLOT_COUNT).enumerate() {
                slots[index] = resolve_raw_form_id(
                    u32::from_le_bytes(chunk.try_into().unwrap()),
                    target_masters,
                    target_plugin,
                    interner,
                );
            }
        }
        FieldValue::Struct(fields) => {
            for (name, value) in fields {
                let Some(name) = interner.resolve(*name) else {
                    continue;
                };
                let Some(index) = template_slot_index(name) else {
                    continue;
                };
                slots[index] = form_key_value(value, target_masters, target_plugin, interner);
            }
        }
        _ => {}
    }
    slots
}

fn template_slot_index(name: &str) -> Option<usize> {
    match normalize_name(name).as_str() {
        "traits" => Some(0),
        "stats" => Some(1),
        "factions" => Some(2),
        "spelllist" => Some(3),
        "aidata" => Some(4),
        "aipackages" => Some(5),
        "modelanimation" => Some(6),
        "basedata" => Some(7),
        "inventory" => Some(8),
        "script" => Some(9),
        "defpackagelist" | "defpacklist" => Some(10),
        "attackdata" => Some(11),
        "keywords" => Some(12),
        _ => None,
    }
}

fn template_slot_name(slot: usize) -> &'static str {
    match slot {
        0 => "traits",
        1 => "stats",
        2 => "factions",
        3 => "spell_list",
        4 => "ai_data",
        5 => "ai_packages",
        6 => "model_animation",
        7 => "base_data",
        8 => "inventory",
        9 => "script",
        10 => "default_package_list",
        11 => "attack_data",
        12 => "keywords",
        _ => "unknown",
    }
}

fn overlay_template_category(
    target: &mut Record,
    source: &Record,
    slot: usize,
    interner: &StringInterner,
) {
    merge_acbs_category(target, source, slot, interner);
    merge_dnam_category(target, source, slot, interner);
    let sigs = category_signatures(slot);
    if sigs.is_empty() {
        return;
    }
    let replacement = source
        .fields
        .iter()
        .filter(|field| sigs.contains(&field.sig.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let mut insert_at = target
        .fields
        .iter()
        .position(|field| sigs.contains(&field.sig.as_str()));
    target
        .fields
        .retain(|field| !sigs.contains(&field.sig.as_str()));
    if replacement.is_empty() {
        return;
    }
    if insert_at.is_none() {
        let replacement_rank = replacement
            .iter()
            .map(|field| npc_field_rank(field.sig.as_str()))
            .min()
            .unwrap_or(u16::MAX);
        insert_at = target
            .fields
            .iter()
            .position(|field| npc_field_rank(field.sig.as_str()) > replacement_rank);
    }
    let insert_at = insert_at
        .unwrap_or(target.fields.len())
        .min(target.fields.len());
    for (offset, field) in replacement.into_iter().enumerate() {
        target.fields.insert(insert_at + offset, field);
    }
}

fn category_signatures(slot: usize) -> &'static [&'static str] {
    match slot {
        0 => &[
            "INAM", "VTCK", "RNAM", "WNAM", "ANAM", "APPR", "OBTE", "OBTF", "OBTS", "STOP", "PNAM",
            "HCLF", "BCLF", "FTST", "QNAM", "MSDK", "MSDV", "TETI", "TEND", "MRSV", "FMRI", "FMRS",
            "FMIN", "NAM4", "NAM5", "NAM6", "NAM7", "MWGT",
        ],
        1 => &["PRPS", "CNAM"],
        2 => &["SNAM", "CRIF"],
        3 => &["SPCT", "SPLO", "PRKZ", "PRKR"],
        4 => &["AIDT", "ZNAM", "GNAM"],
        5 => &["SPOR", "OCOR", "GWOR", "ECOR", "FCPL", "RCLR", "PKID"],
        6 => &[
            "STCP", "NAM8", "CS2H", "CS2K", "CS2D", "CS2E", "CS2F", "CSCR",
        ],
        7 => &[
            "OBND", "PTRN", "DEST", "DAMC", "DSTD", "DSTA", "DMDL", "DMDT", "DMDC", "DMDS", "DSTF",
            "FTYP", "NTRM", "FULL", "SHRT", "DATA", "ATTX",
        ],
        8 => &["COCT", "CNTO", "COED", "DOFT", "SOFT", "PFRN"],
        9 => &["VMAD"],
        10 => &["DPLT"],
        11 => &["ATKR", "ATKD", "ATKE", "ATKW", "ATKS", "ATKT"],
        12 => &["KSIZ", "KWDA"],
        _ => &[],
    }
}

fn merge_acbs_category(
    target: &mut Record,
    source: &Record,
    slot: usize,
    interner: &StringInterner,
) {
    let Some(source_acbs) = source
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "ACBS")
        .map(|field| &field.value)
    else {
        return;
    };
    let Some(target_acbs) = target
        .fields
        .iter_mut()
        .find(|field| field.sig.as_str() == "ACBS")
        .map(|field| &mut field.value)
    else {
        return;
    };
    match (target_acbs, source_acbs) {
        (FieldValue::Bytes(target), FieldValue::Bytes(source))
            if target.len() >= 20 && source.len() >= 20 =>
        {
            let flag_mask = acbs_flag_mask(slot);
            if flag_mask != 0 {
                let target_flags = u32::from_le_bytes(target[0..4].try_into().unwrap());
                let source_flags = u32::from_le_bytes(source[0..4].try_into().unwrap());
                target[0..4].copy_from_slice(
                    &((target_flags & !flag_mask) | (source_flags & flag_mask)).to_le_bytes(),
                );
            }
            if slot == 0 {
                target[12..14].copy_from_slice(&source[12..14]);
            } else if slot == 1 {
                target[4..12].copy_from_slice(&source[4..12]);
                target[16..18].copy_from_slice(&source[16..18]);
            } else if slot == 7 {
                target[18..20].copy_from_slice(&source[18..20]);
            }
        }
        (FieldValue::Struct(target), FieldValue::Struct(source)) => {
            merge_acbs_struct_flags(target, source, slot, interner);
            let names: &[&str] = match slot {
                0 => &["DispositionBase"],
                1 => &[
                    "XPValueOffset",
                    "Level",
                    "LevelMult",
                    "CalcMinLevel",
                    "CalcMaxLevel",
                    "BleedoutOverride",
                ],
                7 => &["UnknownByte9", "UnknownByte10"],
                _ => &[],
            };
            for name in names {
                copy_struct_member(target, source, name, interner);
            }
        }
        _ => {}
    }
}

fn acbs_flag_mask(slot: usize) -> u32 {
    match slot {
        0 => ACBS_TRAITS_FLAG_MASK,
        1 => ACBS_STATS_FLAG_MASK,
        6 => ACBS_MODEL_ANIMATION_FLAG_MASK,
        7 => {
            !(ACBS_TRAITS_FLAG_MASK
                | ACBS_STATS_FLAG_MASK
                | ACBS_MODEL_ANIMATION_FLAG_MASK
                | ACBS_ATTACK_DATA_FLAG_MASK
                | ACBS_CALC_FOR_EACH_TEMPLATE_FLAG)
        }
        11 => ACBS_ATTACK_DATA_FLAG_MASK,
        _ => 0,
    }
}

fn merge_acbs_struct_flags(
    target: &mut [(crate::sym::Sym, FieldValue)],
    source: &[(crate::sym::Sym, FieldValue)],
    slot: usize,
    interner: &StringInterner,
) {
    let Some((_, source_flags)) = source
        .iter()
        .find(|(name, _)| normalized_sym(*name, interner) == "flags")
    else {
        return;
    };
    let Some((_, target_flags)) = target
        .iter_mut()
        .find(|(name, _)| normalized_sym(*name, interner) == "flags")
    else {
        return;
    };
    match (target_flags, source_flags) {
        (FieldValue::List(target_items), FieldValue::List(source_items)) => {
            target_items.retain(|item| {
                !matches!(item, FieldValue::String(value) if interner.resolve(*value).is_some_and(|name| acbs_flag_owner(name) == Some(slot)))
            });
            target_items.extend(source_items.iter().filter(|item| {
                matches!(item, FieldValue::String(value) if interner.resolve(*value).is_some_and(|name| acbs_flag_owner(name) == Some(slot)))
            }).cloned());
        }
        (FieldValue::Uint(target), FieldValue::Uint(source)) => {
            let mask = u64::from(acbs_flag_mask(slot));
            *target = (*target & !mask) | (*source & mask);
        }
        (FieldValue::Int(target), FieldValue::Int(source)) if *target >= 0 && *source >= 0 => {
            let mask = i64::from(acbs_flag_mask(slot));
            *target = (*target & !mask) | (*source & mask);
        }
        _ => {}
    }
}

fn acbs_flag_owner(name: &str) -> Option<usize> {
    match normalize_name(name).as_str() {
        "female" | "ischargenfacepreset" | "swapgenderanims" | "oppositegenderanims" => Some(0),
        "autocalcstats" | "pclevelmult" | "hasbleedoutoverride" => Some(1),
        "hasbasesounddata" => Some(6),
        "calcforeachtemplate" => None,
        "useattackpercentage" => Some(11),
        _ => Some(7),
    }
}

fn merge_dnam_category(
    target: &mut Record,
    source: &Record,
    slot: usize,
    interner: &StringInterner,
) {
    let Some(source_dnam) = source
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "DNAM")
        .map(|field| &field.value)
    else {
        return;
    };
    let Some(target_dnam) = target
        .fields
        .iter_mut()
        .find(|field| field.sig.as_str() == "DNAM")
        .map(|field| &mut field.value)
    else {
        return;
    };
    match (target_dnam, source_dnam) {
        (FieldValue::Bytes(target), FieldValue::Bytes(source))
            if target.len() >= 8 && source.len() >= 8 =>
        {
            match slot {
                0 => target[4..6].copy_from_slice(&source[4..6]),
                1 => target[0..4].copy_from_slice(&source[0..4]),
                7 => target[7] = source[7],
                8 => target[6] = source[6],
                _ => {}
            }
        }
        (FieldValue::Struct(target), FieldValue::Struct(source)) => {
            let names: &[&str] = match slot {
                0 => &["FarAwayModelDistance"],
                1 => &["CalculatedHealth", "CalculatedActionPoints"],
                7 => &["UnknownByte5"],
                8 => &["GearedUpWeapons"],
                _ => &[],
            };
            for name in names {
                copy_struct_member(target, source, name, interner);
            }
        }
        _ => {}
    }
}

fn copy_struct_member(
    target: &mut [(crate::sym::Sym, FieldValue)],
    source: &[(crate::sym::Sym, FieldValue)],
    wanted: &str,
    interner: &StringInterner,
) {
    let wanted = normalize_name(wanted);
    let Some(value) = source
        .iter()
        .find(|(name, _)| normalized_sym(*name, interner) == wanted)
        .map(|(_, value)| value.clone())
    else {
        return;
    };
    if let Some((_, target_value)) = target
        .iter_mut()
        .find(|(name, _)| normalized_sym(*name, interner) == wanted)
    {
        *target_value = value;
    }
}

fn clear_acbs_template_flags(record: &mut Record, interner: &StringInterner) {
    let Some(value) = record
        .fields
        .iter_mut()
        .find(|field| field.sig.as_str() == "ACBS")
        .map(|field| &mut field.value)
    else {
        return;
    };
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= ACBS_TEMPLATE_FLAGS_OFFSET + 2 => {
            let flags = u32::from_le_bytes(bytes[0..4].try_into().unwrap())
                & !ACBS_CALC_FOR_EACH_TEMPLATE_FLAG;
            bytes[0..4].copy_from_slice(&flags.to_le_bytes());
            bytes[ACBS_TEMPLATE_FLAGS_OFFSET..ACBS_TEMPLATE_FLAGS_OFFSET + 2]
                .copy_from_slice(&0u16.to_le_bytes());
        }
        FieldValue::Struct(fields) => {
            if let Some((_, flags)) = fields
                .iter_mut()
                .find(|(name, _)| normalized_sym(*name, interner) == "flags")
            {
                match flags {
                    FieldValue::List(values) => values.retain(|value| {
                        !matches!(value, FieldValue::String(name) if interner.resolve(*name).is_some_and(|name| normalize_name(name) == "calcforeachtemplate"))
                    }),
                    FieldValue::Uint(value) => {
                        *value &= !u64::from(ACBS_CALC_FOR_EACH_TEMPLATE_FLAG)
                    }
                    FieldValue::Int(value) if *value >= 0 => {
                        *value &= !i64::from(ACBS_CALC_FOR_EACH_TEMPLATE_FLAG)
                    }
                    _ => {}
                }
            }
            if let Some((_, flags)) = fields.iter_mut().find(|(name, _)| {
                matches!(
                    normalized_sym(*name, interner).as_str(),
                    "templateflags" | "usetemplateactors"
                )
            }) {
                match flags {
                    FieldValue::List(values) => values.clear(),
                    FieldValue::Uint(value) => *value = 0,
                    FieldValue::Int(value) => *value = 0,
                    FieldValue::Bytes(bytes) => bytes.fill(0),
                    other => *other = FieldValue::Uint(0),
                }
            }
        }
        _ => {}
    }
}

fn npc_is_female(record: &Record, interner: &StringInterner) -> Option<bool> {
    let value = record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "ACBS")
        .map(|field| &field.value)?;
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(u32::from_le_bytes(bytes[0..4].try_into().unwrap()) & 1 != 0)
        }
        FieldValue::Struct(fields) => {
            Some(struct_flags_contains(fields, "Flags", "Female", interner))
        }
        _ => None,
    }
}

fn struct_flags_contains(
    fields: &[(crate::sym::Sym, FieldValue)],
    field_name: &str,
    flag_name: &str,
    interner: &StringInterner,
) -> bool {
    let wanted = normalize_name(field_name);
    fields
        .iter()
        .find(|(name, _)| normalized_sym(*name, interner) == wanted)
        .is_some_and(|(_, value)| match value {
            FieldValue::List(items) => items.iter().any(|item| {
                matches!(item, FieldValue::String(value) if interner.resolve(*value).is_some_and(|name| name.eq_ignore_ascii_case(flag_name)))
            }),
            FieldValue::Uint(flags) => *flags & 1 != 0,
            FieldValue::Int(flags) => *flags & 1 != 0,
            _ => false,
        })
}

fn has_human_face_data(record: &Record) -> bool {
    record.fields.iter().any(|field| {
        matches!(
            field.sig.as_str(),
            "PNAM" | "HCLF" | "BCLF" | "MSDK" | "MSDV" | "FMRI" | "FMRS"
        )
    })
}

fn set_editor_id(record: &mut Record, editor_id: &str, interner: &StringInterner) {
    let value = interner.intern(editor_id);
    record.eid = Some(value);
    if let Some(field) = record
        .fields
        .iter_mut()
        .find(|field| field.sig.as_str() == "EDID")
    {
        field.value = FieldValue::String(value);
        return;
    }
    record.fields.insert(
        0,
        FieldEntry {
            sig: SubrecordSig::from_str("EDID").expect("EDID signature"),
            value: FieldValue::String(value),
        },
    );
}

fn npc_field_rank(sig: &str) -> u16 {
    match sig {
        "EDID" => 0,
        "VMAD" => 1,
        "OBND" => 2,
        "ACBS" => 3,
        "SNAM" => 4,
        "INAM" => 5,
        "VTCK" => 6,
        "TPLT" => 7,
        "LTPT" => 8,
        "LTPC" => 9,
        "TPTA" => 10,
        "RNAM" => 11,
        "SPCT" | "SPLO" => 12,
        "DEST" | "DSTD" | "DMDL" | "DMDT" | "DMDC" | "DMDS" | "DSTF" => 13,
        "WNAM" => 14,
        "ANAM" => 15,
        "ATKR" | "ATKD" | "ATKE" => 16,
        "SPOR" | "OCOR" | "GWOR" | "ECOR" | "FCPL" | "RCLR" => 17,
        "PRKZ" | "PRKR" => 18,
        "PRPS" => 19,
        "FTYP" | "NTRM" => 20,
        "COCT" | "CNTO" => 21,
        "AIDT" => 22,
        "PKID" => 23,
        "KSIZ" | "KWDA" => 24,
        "APPR" => 25,
        "OBTE" | "OBTF" | "OBTS" => 26,
        "CNAM" => 27,
        "FULL" => 28,
        "SHRT" => 29,
        "DATA" => 30,
        "DNAM" => 31,
        "PNAM" | "HCLF" | "BCLF" | "ZNAM" | "GNAM" | "NAM5" | "NAM6" | "NAM7" | "NAM4" | "MWGT"
        | "NAM8" | "CS2H" | "CS2K" | "CS2D" | "CS2E" | "CS2F" | "CSCR" | "PFRN" | "DOFT"
        | "SOFT" | "DPLT" | "CRIF" | "FTST" | "QNAM" | "MSDK" | "MSDV" | "TETI" | "TEND"
        | "MRSV" | "FMRI" | "FMRS" | "FMIN" | "ATTX" => 32,
        _ => 1000,
    }
}

fn is_lvlo_tail(sig: &str) -> bool {
    matches!(sig, "COED" | "CTDA" | "CTDT" | "CIS1" | "CIS2")
}

fn lvlo_reference(
    value: &FieldValue,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    match value {
        FieldValue::Bytes(bytes) => {
            let offset = lvlo_reference_offset(bytes)?;
            resolve_raw_form_id(
                u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()),
                target_masters,
                target_plugin,
                interner,
            )
        }
        FieldValue::Struct(fields) => fields.iter().find_map(|(name, value)| {
            matches!(
                normalized_sym(*name, interner).as_str(),
                "npc" | "reference"
            )
            .then(|| form_key_value(value, target_masters, target_plugin, interner))
            .flatten()
        }),
        FieldValue::FormKey(form_key) if form_key.local != 0 => Some(*form_key),
        _ => None,
    }
}

fn set_lvlo_reference(
    value: &mut FieldValue,
    replacement: FormKey,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> bool {
    match value {
        FieldValue::Bytes(bytes) => {
            let Some(offset) = lvlo_reference_offset(bytes) else {
                return false;
            };
            let Some(raw) =
                encode_raw_form_id(replacement, target_masters, target_plugin, interner)
            else {
                return false;
            };
            bytes[offset..offset + 4].copy_from_slice(&raw.to_le_bytes());
            true
        }
        FieldValue::Struct(fields) => {
            let Some((_, value)) = fields.iter_mut().find(|(name, _)| {
                matches!(
                    normalized_sym(*name, interner).as_str(),
                    "npc" | "reference"
                )
            }) else {
                return false;
            };
            *value = FieldValue::FormKey(replacement);
            true
        }
        FieldValue::FormKey(form_key) => {
            *form_key = replacement;
            true
        }
        _ => false,
    }
}

fn lvlo_reference_offset(bytes: &[u8]) -> Option<usize> {
    if bytes.len() >= 12 {
        Some(4)
    } else if bytes.len() >= 8 {
        Some(2)
    } else {
        None
    }
}

fn form_key_value(
    value: &FieldValue,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(form_key) if form_key.local != 0 => Some(*form_key),
        FieldValue::Uint(raw) if *raw <= u32::MAX as u64 => {
            resolve_raw_form_id(*raw as u32, target_masters, target_plugin, interner)
        }
        FieldValue::Int(raw) if *raw > 0 && *raw <= u32::MAX as i64 => {
            resolve_raw_form_id(*raw as u32, target_masters, target_plugin, interner)
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => resolve_raw_form_id(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            target_masters,
            target_plugin,
            interner,
        ),
        _ => None,
    }
}

fn resolve_raw_form_id(
    raw: u32,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    if raw == 0 {
        return None;
    }
    let index = (raw >> 24) as usize;
    let plugin = target_masters
        .get(index)
        .map(String::as_str)
        .unwrap_or(target_plugin);
    Some(FormKey {
        local: raw & 0x00FF_FFFF,
        plugin: interner.intern(plugin),
    })
}

fn encode_raw_form_id(
    form_key: FormKey,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<u32> {
    if form_key.local == 0 || form_key.local > 0x00FF_FFFF {
        return None;
    }
    let plugin = interner.resolve(form_key.plugin)?;
    let index = if plugin.eq_ignore_ascii_case(target_plugin) {
        target_masters.len()
    } else {
        target_masters
            .iter()
            .position(|master| master.eq_ignore_ascii_case(plugin))?
    };
    (index <= u8::MAX as usize).then(|| ((index as u32) << 24) | form_key.local)
}

fn normalized_sym(symbol: crate::sym::Sym, interner: &StringInterner) -> String {
    interner
        .resolve(symbol)
        .map(normalize_name)
        .unwrap_or_default()
}

fn normalize_name(name: &str) -> String {
    name.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
    use crate::record::RecordFlags;
    use crate::session::open_session;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_native,
    };

    fn field(sig: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        }
    }

    fn record(sig: &str, local: u32, fields: Vec<FieldEntry>, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern("SeventySix.esm"),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: fields.into_iter().collect(),
            warnings: SmallVec::new(),
        }
    }

    fn acbs(female: bool, template_flags: u16) -> FieldEntry {
        let mut bytes = vec![0u8; 20];
        bytes[0..4].copy_from_slice(&u32::from(female).to_le_bytes());
        bytes[ACBS_TEMPLATE_FLAGS_OFFSET..ACBS_TEMPLATE_FLAGS_OFFSET + 2]
            .copy_from_slice(&template_flags.to_le_bytes());
        field("ACBS", FieldValue::Bytes(bytes.into()))
    }

    fn tpta(slots: [Option<FormKey>; SLOT_COUNT], interner: &StringInterner) -> FieldEntry {
        let names = [
            "Traits",
            "Stats",
            "Factions",
            "SpellList",
            "AIData",
            "AIPackages",
            "ModelAnimation",
            "BaseData",
            "Inventory",
            "Script",
            "DefPackageList",
            "AttackData",
            "Keywords",
        ];
        field(
            "TPTA",
            FieldValue::Struct(
                names
                    .into_iter()
                    .zip(slots)
                    .map(|(name, value)| {
                        (
                            interner.intern(name),
                            value
                                .map(FieldValue::FormKey)
                                .unwrap_or(FieldValue::Uint(0)),
                        )
                    })
                    .collect(),
            ),
        )
    }

    fn lvlo(actor: FormKey, interner: &StringInterner) -> FieldEntry {
        field(
            "LVLO",
            FieldValue::Struct(vec![
                (
                    interner.intern("level"),
                    FieldValue::Bytes(1u16.to_le_bytes().as_slice().into()),
                ),
                (
                    interner.intern("unknown_u8_1"),
                    FieldValue::Bytes(vec![0].into()),
                ),
                (
                    interner.intern("unknown_u8_2"),
                    FieldValue::Bytes(vec![0].into()),
                ),
                (interner.intern("NPC"), FieldValue::FormKey(actor)),
                (
                    interner.intern("count"),
                    FieldValue::Bytes(1u16.to_le_bytes().as_slice().into()),
                ),
                (
                    interner.intern("chance_none"),
                    FieldValue::Bytes(vec![0].into()),
                ),
                (
                    interner.intern("unknown_u8_6"),
                    FieldValue::Bytes(vec![0].into()),
                ),
            ]),
        )
    }

    fn dnam(calculated_health: u16) -> FieldEntry {
        let mut bytes = vec![0; 8];
        bytes[0..2].copy_from_slice(&calculated_health.to_le_bytes());
        field("DNAM", FieldValue::Bytes(bytes.into()))
    }

    fn calculated_health(record: &Record, interner: &StringInterner) -> Option<u16> {
        let value = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DNAM")
            .map(|field| &field.value)?;
        match value {
            FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
                Some(u16::from_le_bytes(bytes[0..2].try_into().unwrap()))
            }
            FieldValue::Struct(fields) => fields.iter().find_map(|(name, value)| {
                (normalized_sym(*name, interner) == "calculatedhealth")
                    .then(|| match value {
                        FieldValue::Uint(value) => (*value).try_into().ok(),
                        FieldValue::Int(value) if *value >= 0 => (*value).try_into().ok(),
                        FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
                            Some(u16::from_le_bytes(bytes[0..2].try_into().unwrap()))
                        }
                        _ => None,
                    })
                    .flatten()
            }),
            _ => None,
        }
    }

    #[test]
    fn responder_variant_keeps_role_inventory_and_takes_selected_face() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let role = FormKey {
            local: 0x653237,
            plugin,
        };
        let faces = FormKey {
            local: 0x6529FB,
            plugin,
        };
        let role_template = FormKey {
            local: 0x65322F,
            plugin,
        };
        let selected_face = FormKey {
            local: 0x6529FE,
            plugin,
        };
        let mut slots = [None; SLOT_COUNT];
        slots[TRAITS_SLOT] = Some(faces);
        slots[2] = Some(role_template);
        slots[4] = Some(role_template);
        slots[8] = Some(role_template);
        slots[12] = Some(role_template);
        let base = record(
            "NPC_",
            role.local,
            vec![acbs(true, 0x1115), tpta(slots, &interner)],
            &interner,
        );
        let face = record(
            "NPC_",
            selected_face.local,
            vec![
                acbs(true, 0),
                field("RNAM", FieldValue::FormKey(FormKey { local: 1, plugin })),
                field("PNAM", FieldValue::FormKey(FormKey { local: 2, plugin })),
            ],
            &interner,
        );
        let role_data = record(
            "NPC_",
            role_template.local,
            vec![
                acbs(false, 0),
                field("SNAM", FieldValue::FormKey(FormKey { local: 3, plugin })),
                field("AIDT", FieldValue::Bytes(vec![1, 2, 3].into())),
                field("COCT", FieldValue::Uint(1)),
                field("CNTO", FieldValue::FormKey(FormKey { local: 4, plugin })),
                field("KSIZ", FieldValue::Uint(1)),
                field("KWDA", FieldValue::FormKey(FormKey { local: 5, plugin })),
            ],
            &interner,
        );

        let mut output = base.clone();
        overlay_template_category(&mut output, &face, TRAITS_SLOT, &interner);
        for slot in [2, 4, 8, 12] {
            overlay_template_category(&mut output, &role_data, slot, &interner);
        }
        output
            .fields
            .retain(|entry| !matches!(entry.sig.as_str(), "TPTA" | "TPLT"));
        clear_acbs_template_flags(&mut output, &interner);

        assert!(
            output
                .fields
                .iter()
                .any(|field| field.sig.as_str() == "PNAM")
        );
        assert!(
            output
                .fields
                .iter()
                .any(|field| field.sig.as_str() == "CNTO")
        );
        assert!(
            output
                .fields
                .iter()
                .any(|field| field.sig.as_str() == "SNAM")
        );
        assert!(
            output
                .fields
                .iter()
                .any(|field| field.sig.as_str() == "AIDT")
        );
        assert!(
            output
                .fields
                .iter()
                .any(|field| field.sig.as_str() == "KWDA")
        );
        assert!(
            !output
                .fields
                .iter()
                .any(|field| field.sig.as_str() == "TPTA")
        );
        let FieldValue::Bytes(acbs) = &output
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "ACBS")
            .unwrap()
            .value
        else {
            panic!("ACBS should be raw bytes");
        };
        assert_eq!(
            u16::from_le_bytes(
                acbs[ACBS_TEMPLATE_FLAGS_OFFSET..ACBS_TEMPLATE_FLAGS_OFFSET + 2]
                    .try_into()
                    .unwrap()
            ),
            0
        );
        assert!(npc_is_female(&output, &interner).unwrap());
    }

    #[test]
    fn lvlo_retarget_preserves_conditions_and_count() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let original = FormKey { local: 1, plugin };
        let clone = FormKey { local: 2, plugin };
        let mut list = record(
            "LVLN",
            3,
            vec![
                field("LLCT", FieldValue::Uint(1)),
                lvlo(original, &interner),
                field("CTDA", FieldValue::Bytes(vec![0; 32].into())),
            ],
            &interner,
        );
        let mut replacement = list.fields[1..3].to_vec();
        assert!(set_lvlo_reference(
            &mut replacement[0].value,
            clone,
            &[],
            "SeventySix.esm",
            &interner,
        ));
        list.fields = vec![
            list.fields[0].clone(),
            replacement[0].clone(),
            replacement[1].clone(),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            lvlo_reference(&list.fields[1].value, &[], "SeventySix.esm", &interner,),
            Some(clone)
        );
        assert_eq!(list.fields[2].sig.as_str(), "CTDA");
        assert_eq!(list.fields[0].value, FieldValue::Uint(1));
    }

    #[test]
    fn writes_durable_facegen_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let config = FixupConfig {
            mod_path: Some(temp.path().to_path_buf()),
            ..FixupConfig::default()
        };
        write_manifest(
            &config,
            vec![FacegenAlias {
                source_plugin: "SeventySix.esm".into(),
                source_local: 0x6529FE,
                target_plugin: "SeventySix.esm".into(),
                target_local: 0xF00001,
            }],
            vec![MaterializedNpcEntry {
                source_actor_plugin: "SeventySix.esm".into(),
                source_actor_local: 0x653237,
                traits_source_plugin: "SeventySix.esm".into(),
                traits_source_local: 0x6529FE,
                clone_local: 0xF00001,
            }],
        )
        .unwrap();
        let decoded: FacegenAliasManifest =
            serde_json::from_slice(&fs::read(manifest_path(temp.path())).unwrap()).unwrap();
        assert_eq!(decoded.aliases.len(), 1);
        assert_eq!(decoded.materializations.len(), 1);
    }

    #[test]
    fn reads_template_leaf_from_target_master_handle() {
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let master_name = "MaterializedNpcMasterReadTest.esm";
        let target_name = "MaterializedNpcTargetReadTest.esp";
        let master = plugin_handle_new_native(master_name, Some("fo4")).unwrap();
        let target = plugin_handle_new_native(target_name, Some("fo4")).unwrap();
        plugin_handle_add_master_native(target, master_name, None).unwrap();
        let leaf = FormKey {
            local: 0x1234,
            plugin: interner.intern(master_name),
        };
        {
            let mut session = open_session(master, None).unwrap();
            session
                .add_records(
                    vec![Record::new(SigCode::from_str("NPC_").unwrap(), leaf)],
                    &schema,
                    &interner,
                )
                .unwrap();
            session.flush_pending_effects();
        }

        {
            let mut session = open_session(target, None).unwrap();
            let resolved = read_target_or_master_record(
                &mut session,
                &schema,
                &interner,
                interner.intern(target_name),
                &[master_name.to_string()],
                &[master],
                leaf,
            )
            .expect("master NPC should resolve");
            assert_eq!(resolved.form_key, leaf);
            assert_eq!(resolved.sig.as_str(), "NPC_");
        }
        assert!(plugin_handle_close_native(target));
        assert!(plugin_handle_close_native(master));
    }

    #[test]
    fn materializes_master_actor_from_source_template_slots() {
        let interner = StringInterner::new();
        let source_name = "SourceAwareMaterializedSource.esm";
        let master_name = "SourceAwareMaterializedMaster.esm";
        let target_name = "SourceAwareMaterializedTarget.esm";
        let source_plugin = interner.intern(source_name);
        let master_plugin = interner.intern(master_name);
        let target_plugin = interner.intern(target_name);
        let source = plugin_handle_new_native(source_name, Some("fo76")).unwrap();
        let master = plugin_handle_new_native(master_name, Some("fo4")).unwrap();
        let target = plugin_handle_new_native(target_name, Some("fo4")).unwrap();
        plugin_handle_add_master_native(target, master_name, None).unwrap();

        let source_actor = FormKey {
            local: 0x1001,
            plugin: source_plugin,
        };
        let source_template_list = FormKey {
            local: 0x1002,
            plugin: source_plugin,
        };
        let master_actor = FormKey {
            local: 0x2001,
            plugin: master_plugin,
        };
        let master_traits = FormKey {
            local: 0x2002,
            plugin: master_plugin,
        };
        let master_script = FormKey {
            local: 0x2003,
            plugin: master_plugin,
        };
        let outer = FormKey {
            local: 0x3001,
            plugin: target_plugin,
        };
        let target_template_list = FormKey {
            local: 0x3002,
            plugin: target_plugin,
        };
        let target_template_actor = FormKey {
            local: 0x3003,
            plugin: target_plugin,
        };

        let mut source_actor_record = record(
            "NPC_",
            source_actor.local,
            vec![
                acbs(false, (1 << TRAITS_SLOT) | (1 << 1) | (1 << SCRIPT_SLOT)),
                field("TPLT", FieldValue::FormKey(source_template_list)),
            ],
            &interner,
        );
        source_actor_record.form_key = source_actor;
        let source_schema = AuthoringSchema::for_game("fo76").unwrap();
        {
            let mut session = open_session(source, None).unwrap();
            session
                .add_records(vec![source_actor_record], &source_schema, &interner)
                .unwrap();
            session.flush_pending_effects();
        }

        let mut master_slots = [None; SLOT_COUNT];
        master_slots[TRAITS_SLOT] = Some(master_traits);
        master_slots[SCRIPT_SLOT] = Some(master_script);
        let mut master_actor_record = record(
            "NPC_",
            master_actor.local,
            vec![
                acbs(false, (1 << TRAITS_SLOT) | (1 << SCRIPT_SLOT)),
                tpta(master_slots, &interner),
                dnam(1),
            ],
            &interner,
        );
        master_actor_record.form_key = master_actor;
        let mut master_traits_record =
            record("NPC_", master_traits.local, vec![acbs(false, 0)], &interner);
        master_traits_record.form_key = master_traits;
        let mut master_script_record = record(
            "NPC_",
            master_script.local,
            vec![
                acbs(false, 0),
                field("VMAD", FieldValue::Bytes(vec![6, 0, 2, 0, 0, 0].into())),
            ],
            &interner,
        );
        master_script_record.form_key = master_script;
        let target_schema = AuthoringSchema::for_game("fo4").unwrap();
        {
            let mut session = open_session(master, None).unwrap();
            session
                .add_records(
                    vec![
                        master_actor_record,
                        master_traits_record,
                        master_script_record,
                    ],
                    &target_schema,
                    &interner,
                )
                .unwrap();
            session.flush_pending_effects();
        }

        let mut outer_record = record(
            "LVLN",
            outer.local,
            vec![
                field("LLCT", FieldValue::Uint(1)),
                lvlo(master_actor, &interner),
            ],
            &interner,
        );
        outer_record.form_key = outer;
        let mut template_list_record = record(
            "LVLN",
            target_template_list.local,
            vec![
                field("LLCT", FieldValue::Uint(1)),
                lvlo(target_template_actor, &interner),
            ],
            &interner,
        );
        template_list_record.form_key = target_template_list;
        let mut template_actor_record = record(
            "NPC_",
            target_template_actor.local,
            vec![acbs(false, 0), dnam(321)],
            &interner,
        );
        template_actor_record.form_key = target_template_actor;
        {
            let mut session = open_session(target, None).unwrap();
            session
                .add_records(
                    vec![outer_record, template_list_record, template_actor_record],
                    &target_schema,
                    &interner,
                )
                .unwrap();
            session.flush_pending_effects();
        }

        let temp = tempfile::tempdir().unwrap();
        let config = FixupConfig {
            is_whole_plugin: true,
            mod_path: Some(temp.path().to_path_buf()),
            target_master_handle_ids: vec![master],
            ..FixupConfig::default()
        };
        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: target_name.to_string(),
                source_plugin_name: source_name.to_string(),
                target_master_names: vec![master_name.to_string()],
                ..MapperOptions::default()
            },
            &interner,
        );
        mapper.add_mapping(source_actor, master_actor);
        mapper.add_mapping(source_template_list, target_template_list);

        {
            let mut session = open_session(target, Some(source)).unwrap();
            let report = MaterializeLeveledTemplateNpcsFixup
                .run_with_session(&mut session, &mut mapper, &config)
                .unwrap();
            session.flush_pending_effects();
            let message = report
                .message
                .and_then(|message| interner.resolve(message))
                .unwrap_or("<no report>");
            assert_eq!(report.records_changed, 1, "{message}");
            assert_eq!(report.records_added, 2, "{message}");
            assert!(message.contains("scripted_skipped=0"), "{message}");
            assert!(message.contains("source_template_overrides=1"), "{message}");
            assert!(message.contains("lists_scanned=2"), "{message}");
            assert!(message.contains("entries_seen=2"), "{message}");
            assert!(message.contains("actors_materialized=1"), "{message}");
            let diagnostics = report
                .diagnostics
                .iter()
                .filter_map(|warning| interner.resolve(*warning))
                .collect::<Vec<_>>();
            assert!(
                diagnostics.iter().any(|diagnostic| {
                    diagnostic.contains("decision:source_template_override")
                        && diagnostic.contains(master_name)
                        && diagnostic.contains(source_name)
                }),
                "{diagnostics:?}"
            );

            let rewritten = session
                .record_decoded(&outer, &target_schema, &interner)
                .unwrap();
            let variant_list = rewritten
                .fields
                .iter()
                .find(|entry| entry.sig.as_str() == "LVLO")
                .and_then(|entry| {
                    lvlo_reference(
                        &entry.value,
                        &[master_name.to_string()],
                        target_name,
                        &interner,
                    )
                })
                .unwrap();
            assert_ne!(variant_list, master_actor);
            let generated = session
                .record_decoded(&variant_list, &target_schema, &interner)
                .unwrap();
            let clone = generated
                .fields
                .iter()
                .find(|entry| entry.sig.as_str() == "LVLO")
                .and_then(|entry| {
                    lvlo_reference(
                        &entry.value,
                        &[master_name.to_string()],
                        target_name,
                        &interner,
                    )
                })
                .unwrap();
            let clone = session
                .record_decoded(&clone, &target_schema, &interner)
                .unwrap();
            assert!(
                !clone
                    .fields
                    .iter()
                    .any(|field| matches!(field.sig.as_str(), "VMAD" | "TPTA" | "TPLT"))
            );
            assert_eq!(calculated_health(&clone, &interner), Some(321));
        }

        assert!(plugin_handle_close_native(target));
        assert!(plugin_handle_close_native(master));
        assert!(plugin_handle_close_native(source));
    }

    #[test]
    fn logs_scripted_actor_skip_with_identity_and_reason() {
        let interner = StringInterner::new();
        let source_name = "ScriptedSkipSource.esm";
        let target_name = "ScriptedSkipTarget.esm";
        let target_plugin = interner.intern(target_name);
        let source = plugin_handle_new_native(source_name, Some("fo76")).unwrap();
        let target = plugin_handle_new_native(target_name, Some("fo4")).unwrap();
        let outer = FormKey {
            local: 0x1000,
            plugin: target_plugin,
        };
        let actor = FormKey {
            local: 0x1001,
            plugin: target_plugin,
        };
        let traits = FormKey {
            local: 0x1002,
            plugin: target_plugin,
        };
        let mut slots = [None; SLOT_COUNT];
        slots[TRAITS_SLOT] = Some(traits);
        let mut actor_record = record(
            "NPC_",
            actor.local,
            vec![
                acbs(false, 1 << TRAITS_SLOT),
                tpta(slots, &interner),
                field("VMAD", FieldValue::Bytes(vec![6, 0, 2, 0, 0, 0].into())),
            ],
            &interner,
        );
        actor_record.form_key = actor;
        set_editor_id(&mut actor_record, "ScriptedTemplateActor", &interner);
        let mut outer_record = record(
            "LVLN",
            outer.local,
            vec![field("LLCT", FieldValue::Uint(1)), lvlo(actor, &interner)],
            &interner,
        );
        outer_record.form_key = outer;
        let mut traits_record = record("NPC_", traits.local, vec![acbs(false, 0)], &interner);
        traits_record.form_key = traits;
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        {
            let mut session = open_session(target, None).unwrap();
            session
                .add_records(
                    vec![outer_record, actor_record, traits_record],
                    &schema,
                    &interner,
                )
                .unwrap();
            session.flush_pending_effects();
        }

        let config = FixupConfig {
            is_whole_plugin: true,
            ..FixupConfig::default()
        };
        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: target_name.to_string(),
                source_plugin_name: source_name.to_string(),
                ..MapperOptions::default()
            },
            &interner,
        );
        {
            let mut session = open_session(target, Some(source)).unwrap();
            let report = MaterializeLeveledTemplateNpcsFixup
                .run_with_session(&mut session, &mut mapper, &config)
                .unwrap();
            let message = report
                .message
                .and_then(|message| interner.resolve(message))
                .unwrap_or("<no report>");
            assert_eq!(report.records_changed, 0, "{message}");
            assert_eq!(report.records_added, 0, "{message}");
            assert!(message.contains("scripted_skipped=1"), "{message}");
            assert!(report.warnings.iter().any(|warning| {
                interner.resolve(*warning).is_some_and(|diagnostic| {
                    diagnostic.contains("skip:scripted_actor")
                        && diagnostic.contains("ScriptedTemplateActor")
                        && diagnostic.contains("reason=target_direct_vmad")
                })
            }));
        }

        assert!(plugin_handle_close_native(target));
        assert!(plugin_handle_close_native(source));
    }

    #[test]
    fn replaces_one_role_entry_with_one_concrete_variant_list() {
        let interner = StringInterner::new();
        let plugin_name = "SeventySix.esm";
        let plugin = interner.intern(plugin_name);
        let target = plugin_handle_new_native(plugin_name, Some("fo4")).unwrap();
        let outer = FormKey {
            local: 0x1000,
            plugin,
        };
        let role = FormKey {
            local: 0x1001,
            plugin,
        };
        let faces = FormKey {
            local: 0x1002,
            plugin,
        };
        let face_a = FormKey {
            local: 0x1003,
            plugin,
        };
        let face_b = FormKey {
            local: 0x1004,
            plugin,
        };
        let role_template = FormKey {
            local: 0x1005,
            plugin,
        };
        let mut slots = [None; SLOT_COUNT];
        slots[TRAITS_SLOT] = Some(faces);
        slots[1] = Some(faces);
        slots[8] = Some(role_template);
        let mut role_record = record(
            "NPC_",
            role.local,
            vec![acbs(true, 0x0103), tpta(slots, &interner), dnam(10)],
            &interner,
        );
        set_editor_id(&mut role_record, "EncResponderRole", &interner);
        let records = vec![
            record(
                "LVLN",
                outer.local,
                vec![
                    field("LLCT", FieldValue::Uint(1)),
                    lvlo(role, &interner),
                    field("CTDA", FieldValue::Bytes(vec![0; 32].into())),
                ],
                &interner,
            ),
            role_record,
            record(
                "LVLN",
                faces.local,
                vec![
                    field("LLCT", FieldValue::Uint(2)),
                    lvlo(face_a, &interner),
                    lvlo(face_b, &interner),
                ],
                &interner,
            ),
            record(
                "NPC_",
                face_a.local,
                vec![
                    acbs(true, 0),
                    dnam(111),
                    field("PNAM", FieldValue::FormKey(face_a)),
                ],
                &interner,
            ),
            record(
                "NPC_",
                face_b.local,
                vec![
                    acbs(true, 0),
                    dnam(222),
                    field("PNAM", FieldValue::FormKey(face_b)),
                ],
                &interner,
            ),
            record(
                "NPC_",
                role_template.local,
                vec![
                    acbs(false, 0),
                    field("COCT", FieldValue::Uint(1)),
                    field("CNTO", FieldValue::FormKey(role_template)),
                ],
                &interner,
            ),
        ];
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        {
            let mut session = open_session(target, None).unwrap();
            session.add_records(records, &schema, &interner).unwrap();
            session.flush_pending_effects();
        }
        let temp = tempfile::tempdir().unwrap();
        let config = FixupConfig {
            is_whole_plugin: true,
            mod_path: Some(temp.path().to_path_buf()),
            ..FixupConfig::default()
        };
        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: plugin_name.to_string(),
                ..MapperOptions::default()
            },
            &interner,
        );
        {
            let mut session = open_session(target, None).unwrap();
            let decoded_role = session.record_decoded(&role, &schema, &interner).unwrap();
            let decoded_slots = template_slots(&decoded_role, &[], plugin_name, &interner);
            assert_eq!(decoded_slots[TRAITS_SLOT], Some(faces));
            assert_eq!(decoded_slots[1], Some(faces));
            assert_eq!(decoded_slots[8], Some(role_template));
            let decoded_outer = session.record_decoded(&outer, &schema, &interner).unwrap();
            let decoded_entry = decoded_outer
                .fields
                .iter()
                .find(|entry| entry.sig.as_str() == "LVLO")
                .unwrap();
            assert_eq!(
                lvlo_reference(&decoded_entry.value, &[], plugin_name, &interner),
                Some(role),
                "{:?}",
                decoded_entry.value
            );
            let report = MaterializeLeveledTemplateNpcsFixup
                .run_with_session(&mut session, &mut mapper, &config)
                .unwrap();
            session.flush_pending_effects();
            let message = report
                .message
                .and_then(|message| interner.resolve(message))
                .unwrap_or("<no report>");
            assert_eq!(report.records_changed, 1, "{message}");
            assert_eq!(report.records_added, 3);

            let rewritten = session.record_decoded(&outer, &schema, &interner).unwrap();
            let entries = rewritten
                .fields
                .iter()
                .filter(|entry| entry.sig.as_str() == "LVLO")
                .collect::<Vec<_>>();
            assert_eq!(entries.len(), 1, "outer role weighting must stay one entry");
            let variant_list =
                lvlo_reference(&entries[0].value, &[], plugin_name, &interner).unwrap();
            assert_ne!(variant_list, role);

            let generated = session
                .record_decoded(&variant_list, &schema, &interner)
                .unwrap_or_else(|error| {
                    panic!(
                        "variant list {variant_list:?} from {:?}: {error}",
                        entries[0].value
                    )
                });
            let clones = generated
                .fields
                .iter()
                .filter(|entry| entry.sig.as_str() == "LVLO")
                .filter_map(|entry| lvlo_reference(&entry.value, &[], plugin_name, &interner))
                .collect::<Vec<_>>();
            assert_eq!(clones.len(), 2);
            let mut face_health = Vec::new();
            for clone in clones {
                let clone = session.record_decoded(&clone, &schema, &interner).unwrap();
                assert!(
                    !clone
                        .fields
                        .iter()
                        .any(|entry| matches!(entry.sig.as_str(), "TPTA" | "TPLT"))
                );
                assert!(
                    clone
                        .fields
                        .iter()
                        .any(|entry| entry.sig.as_str() == "CNTO")
                );
                let face = clone
                    .fields
                    .iter()
                    .find(|entry| entry.sig.as_str() == "PNAM")
                    .and_then(|entry| form_key_value(&entry.value, &[], plugin_name, &interner))
                    .unwrap();
                face_health.push((face.local, calculated_health(&clone, &interner).unwrap()));
            }
            face_health.sort_unstable();
            assert_eq!(face_health, vec![(face_a.local, 111), (face_b.local, 222)]);
        }
        assert!(plugin_handle_close_native(target));
    }
}
