//! Rewrite FormKey references in parsed subgraph blocks.
//!
//! Mapped refs become the mapped FK. Unmapped refs into a source plugin are
//! dropped, since they would dangle at deserialize; unmapped base-game refs stay.

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
        // Keep mapped refs even when the output shares the source plugin's name
        // (whole-plugin FO76->FO4 writes `SeventySix.esm`); comparing the mapped
        // plugin to `source_plugins` would drop every in-place FO76 keyword.
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

    /// Mapped refs are kept even when the output shares the source plugin's
    /// name; unmapped source refs would dangle at deserialize and are dropped;
    /// unmapped base-game refs pass through.
    #[test]
    fn remaps_keeps_and_drops_keyword_refs() {
        let mut mapper_interner = StringInterner::new();
        let source = mapper_interner.intern("SeventySix.esm");
        let output = mapper_interner.intern("Output.esp");
        let base = mapper_interner.intern("Fallout4.esm");
        let fk = |local, plugin| FormKey { local, plugin };
        let remapped_src = fk(0x100, source);
        let remapped_out = fk(0x800, output);
        let same_named = fk(0x568776, source);
        let unmapped_src = fk(0x200, source);
        let base_ref = fk(0x12345, base);
        let mut mapper = FormKeyMapper::new(
            std::iter::empty::<(Sym, FormKey, SigCode)>(),
            MapperOptions::default(),
            &mut mapper_interner,
        );
        mapper.add_mapping(remapped_src, remapped_out);
        mapper.add_mapping(same_named, same_named);
        let source_plugins: FxHashSet<Sym> = [source].into_iter().collect();
        assert!(rewrite_subgraph_block_formkeys(vec![], &mapper, &source_plugins).is_empty());

        let block = |subgraph_keywords, target_keywords| SubgraphBlock {
            behaviour_graph: mapper.interner.intern("X.hkx"),
            paths: vec![],
            subgraph_keywords,
            target_keywords,
            flags_bytes: None,
        };
        let out = rewrite_subgraph_block_formkeys(
            vec![
                block(vec![], vec![]),
                block(
                    vec![remapped_src, unmapped_src, base_ref],
                    vec![unmapped_src, same_named],
                ),
            ],
            &mapper,
            &source_plugins,
        );
        assert_eq!(out.len(), 2);
        assert!(out[0].subgraph_keywords.is_empty() && out[0].target_keywords.is_empty());
        assert_eq!(out[1].subgraph_keywords, vec![remapped_out, base_ref]);
        assert_eq!(out[1].target_keywords, vec![same_named]);
    }
}
