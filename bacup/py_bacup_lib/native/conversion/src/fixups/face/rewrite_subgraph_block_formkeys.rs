//! Rewrite FormKey references in parsed subgraph blocks.
//!

//! # What this does
//! For each block's `subgraph_keywords` and `target_keywords` lists, remap
//! every FormKey through the `FormKeyMapper`:
//! - A ref that SUCCESSFULLY maps (`mapper.lookup` is `Some`) points at a
//!   converted output record and is **kept** as the mapped FK.
//! - A ref with NO mapping that still points at a source plugin is **dropped**
//!   — it was never carried into the output, so it would dangle an invalid
//!   master reference at deserialize time. An unmapped ref at a non-source
//!   plugin (a base-game master) is kept as-is.
//!

use rustc_hash::FxHashSet;

use crate::fixups::face::build_additive_race_record::SubgraphBlock;
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::FormKey;
use crate::sym::Sym;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Rewrite FormKey refs in each block via the mapper. Mapped refs are always
/// kept; an UNMAPPED ref still pointing at a `source_plugins` plugin is dropped.
pub fn rewrite_subgraph_block_formkeys(
    blocks: Vec<SubgraphBlock>,
    mapper: &FormKeyMapper,
    source_plugins: &FxHashSet<Sym>,
) -> Vec<SubgraphBlock> {
    blocks
        .into_iter()
        .map(|mut b| {
            b.subgraph_keywords = remap_drop_source(&b.subgraph_keywords, mapper, source_plugins);
            b.target_keywords = remap_drop_source(&b.target_keywords, mapper, source_plugins);
            b
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Private helper
// ---------------------------------------------------------------------------

fn remap_drop_source(
    fks: &[FormKey],
    mapper: &FormKeyMapper,
    source_plugins: &FxHashSet<Sym>,
) -> Vec<FormKey> {
    let mut out = Vec::with_capacity(fks.len());
    for fk in fks {
        // A ref that SUCCESSFULLY maps points at a converted output record and
        // must be kept — even when the output plugin shares its name with a
        // source plugin (whole-plugin FO76->FO4 regen writes `SeventySix.esm`,
        // the same name as the source). Comparing the mapped plugin against
        // `source_plugins` would then drop every FO76-unique keyword that was
        // converted in-place (gauss pistol, etc.). Only an UNMAPPED ref that
        // still dangles at a source plugin is dropped — it was never carried
        // into the output, so it would be an invalid reference at deserialize.
        match mapper.lookup(*fk) {
            Some(mapped) => out.push(mapped),
            None => {
                if !source_plugins.contains(&fk.plugin) {
                    out.push(*fk);
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
    use crate::ids::SigCode;
    use crate::sym::StringInterner;

    /// empty input → empty output.
    #[test]
    fn empty_input_returns_empty() {
        let mut interner = StringInterner::new();
        let mapper = FormKeyMapper::new(
            std::iter::empty::<(Sym, FormKey, SigCode)>(),
            MapperOptions::default(),
            &mut interner,
        );
        let out = rewrite_subgraph_block_formkeys(vec![], &mapper, &FxHashSet::default());
        assert!(out.is_empty());
    }

    /// block with no keywords passes through unchanged.
    #[test]
    fn block_without_keywords_unchanged() {
        let mut interner = StringInterner::new();
        let bg = interner.intern("X.hkx");
        let mapper = FormKeyMapper::new(
            std::iter::empty::<(Sym, FormKey, SigCode)>(),
            MapperOptions::default(),
            &mut interner,
        );
        let block = SubgraphBlock {
            behaviour_graph: bg,
            paths: vec![],
            subgraph_keywords: vec![],
            target_keywords: vec![],
            flags_bytes: None,
        };
        let out = rewrite_subgraph_block_formkeys(vec![block], &mapper, &FxHashSet::default());
        assert_eq!(out.len(), 1);
        assert!(out[0].subgraph_keywords.is_empty());
        assert!(out[0].target_keywords.is_empty());
    }

    /// an UNMAPPED ref still dangling at a source plugin is dropped.
    /// (A source record that was never converted has no mapper entry; leaving
    /// the ref would dangle an invalid master reference at deserialize time.)
    #[test]
    fn drops_unmapped_source_plugin_refs() {
        let mut mapper_interner = StringInterner::new();
        let source_plugin_sym = mapper_interner.intern("SeventySix.esm");
        let src_fk = FormKey {
            local: 0x100,
            plugin: source_plugin_sym,
        };
        // No mapping for src_fk — it was never converted.
        let mapper = FormKeyMapper::new(
            std::iter::empty::<(Sym, FormKey, SigCode)>(),
            MapperOptions::default(),
            &mut mapper_interner,
        );

        let block = SubgraphBlock {
            behaviour_graph: mapper.interner.intern("X.hkx"),
            paths: vec![],
            subgraph_keywords: vec![src_fk],
            target_keywords: vec![src_fk],
            flags_bytes: None,
        };
        let source_plugins: FxHashSet<Sym> = [source_plugin_sym].into_iter().collect();
        let out = rewrite_subgraph_block_formkeys(vec![block], &mapper, &source_plugins);
        assert_eq!(out.len(), 1);
        assert!(out[0].subgraph_keywords.is_empty());
        assert!(out[0].target_keywords.is_empty());
    }

    /// refs that remap to non-source plugin are kept.
    #[test]
    fn keeps_remapped_refs() {
        let mut mapper_interner = StringInterner::new();
        let source_plugin_sym = mapper_interner.intern("SeventySix.esm");
        let target_plugin_sym = mapper_interner.intern("Output.esp");
        let src_fk = FormKey {
            local: 0x100,
            plugin: source_plugin_sym,
        };
        let tgt_fk = FormKey {
            local: 0x800,
            plugin: target_plugin_sym,
        };
        let mut mapper = FormKeyMapper::new(
            std::iter::empty::<(Sym, FormKey, SigCode)>(),
            MapperOptions::default(),
            &mut mapper_interner,
        );
        mapper.add_mapping(src_fk, tgt_fk);

        let block = SubgraphBlock {
            behaviour_graph: mapper.interner.intern("X.hkx"),
            paths: vec![],
            subgraph_keywords: vec![src_fk],
            target_keywords: vec![src_fk],
            flags_bytes: None,
        };
        let source_plugins: FxHashSet<Sym> = [source_plugin_sym].into_iter().collect();
        let out = rewrite_subgraph_block_formkeys(vec![block], &mapper, &source_plugins);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].subgraph_keywords, vec![tgt_fk]);
        assert_eq!(out[0].target_keywords, vec![tgt_fk]);
    }

    /// When the OUTPUT plugin is named the same as the SOURCE (e.g.
    /// `SeventySix.esm` -> `SeventySix.esm`), a keyword that SUCCESSFULLY maps to
    /// a converted output record must be KEPT even though the output plugin name
    /// is also in `source_plugins`.
    #[test]
    fn keeps_ref_mapped_into_same_named_output_plugin() {
        let mut mapper_interner = StringInterner::new();
        // Source AND output share the name (whole-plugin FO76->FO4 regen).
        let plugin_sym = mapper_interner.intern("SeventySix.esm");
        let src_fk = FormKey {
            local: 0x568776,
            plugin: plugin_sym,
        };
        // Converted output keyword: objid preserved, still in SeventySix.esm.
        let out_fk = FormKey {
            local: 0x568776,
            plugin: plugin_sym,
        };
        let mut mapper = FormKeyMapper::new(
            std::iter::empty::<(Sym, FormKey, SigCode)>(),
            MapperOptions::default(),
            &mut mapper_interner,
        );
        mapper.add_mapping(src_fk, out_fk);

        let block = SubgraphBlock {
            behaviour_graph: mapper.interner.intern("X.hkx"),
            paths: vec![],
            subgraph_keywords: vec![],
            target_keywords: vec![src_fk],
            flags_bytes: None,
        };
        let source_plugins: FxHashSet<Sym> = [plugin_sym].into_iter().collect();
        let out = rewrite_subgraph_block_formkeys(vec![block], &mapper, &source_plugins);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].target_keywords, vec![out_fk]);
    }

    /// unmapped ref pointing at a non-source plugin is kept as-is.
    #[test]
    fn unmapped_non_source_ref_passes_through() {
        let mut mapper_interner = StringInterner::new();
        let source_plugin_sym = mapper_interner.intern("SeventySix.esm");
        let other_plugin_sym = mapper_interner.intern("Fallout4.esm");
        let other_fk = FormKey {
            local: 0x12345,
            plugin: other_plugin_sym,
        };
        let mapper = FormKeyMapper::new(
            std::iter::empty::<(Sym, FormKey, SigCode)>(),
            MapperOptions::default(),
            &mut mapper_interner,
        );
        let block = SubgraphBlock {
            behaviour_graph: mapper.interner.intern("X.hkx"),
            paths: vec![],
            subgraph_keywords: vec![other_fk],
            target_keywords: vec![],
            flags_bytes: None,
        };
        let source_plugins: FxHashSet<Sym> = [source_plugin_sym].into_iter().collect();
        let out = rewrite_subgraph_block_formkeys(vec![block], &mapper, &source_plugins);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].subgraph_keywords, vec![other_fk]);
    }
}
