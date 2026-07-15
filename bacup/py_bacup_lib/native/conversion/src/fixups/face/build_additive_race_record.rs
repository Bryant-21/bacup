//! Parse / strip / compose canonical-shape RACE subgraph data.
//!

//! # What this module owns
//! The `SubgraphBlock` representation of one canonical subgraph section plus
//! the writer helpers that work against the native pipeline's
//! `Record` / `FieldEntry` / `FieldValue` types. Canonical block grouping is
//! shared with `ck_native`; this module only adapts conversion-owned types.
//!
//! # Why these live together
//! The conversion adapter and `block_to_entries` are inverse operations, and
//! `build_additive_race_record` composes the result of
//! `strip_template_subgraph_fields` with re-serialized blocks. Keeping the
//! round-trip pair colocated keeps the schema mapping (canonical YAML label →
//! subrecord sig) in one place.
//!
//! # Schema mapping
//! Canonical YAML label → FO4 RACE subrecord:
//! - `BehaviourGraph`         → `SGNM` (zstring, repeatable; main block marker)
//! - `Path`                   → `SAPT` (zstring, repeatable)
//! - `SAKD`                   → `SAKD` (formid → KYWD, repeatable;
//!                                       `subgraph_keywords`)
//! - `STKD`                   → `STKD` (formid → KYWD, repeatable;
//!                                       `target_keywords`)
//! - `SRAF`                   → `SRAF` (struct:H,H — role+perspective;
//!                                       carried opaquely as raw bytes,
//!                                       see deviation below)
//! - `SubgraphAdditiveRace`   → `SADD` (formid → RACE)
//!
//! # SRAF opaque carry
//! SRAF struct (`struct:H,H` role+perspective) fields carry as raw bytes
//! (`FieldValue::Bytes`); the fixup never interprets the H,H values, so this
//! is round-trip-safe.

use smallvec::SmallVec;

use crate::ids::{FormKey, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::Sym;
use ck_native::anim_text_data::race_decode::{
    CanonicalSubgraphField, parse_canonical_subgraphs as parse_plain_canonical_subgraphs,
};

// ---------------------------------------------------------------------------
// SubgraphBlock
// ---------------------------------------------------------------------------

/// A canonical subgraph block parsed out of a RACE record.
///
/// FO4 records commonly store keyword rows immediately before the `SGNM`
/// behaviour-graph row (`SAKD*`, `STKD*`, `SGNM`, `SAPT*`, `SRAF`). The parser
/// also accepts older synthesized rows where keywords follow `SGNM`.
#[derive(Debug, Clone, PartialEq)]
pub struct SubgraphBlock {
    /// `SGNM` value — interned behaviour-graph path.
    pub behaviour_graph: Sym,
    /// `SAPT` values — interned animation directory paths, in source order.
    pub paths: Vec<Sym>,
    /// `SAKD` values — subgraph keyword FormKeys, in source order.
    pub subgraph_keywords: Vec<FormKey>,
    /// `STKD` values — target-keyword FormKeys, in source order.
    pub target_keywords: Vec<FormKey>,
    /// `SRAF` opaque struct bytes — typically 4 bytes (`H,H`). `None` when
    /// the source race had no SRAF for this block. Uses the same inline
    /// capacity as `FieldValue::Bytes` so round-trip cloning avoids
    /// reallocation.
    pub flags_bytes: Option<SmallVec<[u8; 32]>>,
}

// ---------------------------------------------------------------------------
// parse_canonical_subgraphs
// ---------------------------------------------------------------------------

/// Parse a RACE record's `fields` list into a flat sequence of subgraph blocks.
pub fn parse_canonical_subgraphs(record: &Record) -> Vec<SubgraphBlock> {
    let fields = record.fields.iter().filter_map(|entry| {
        let value = match (entry.sig.as_str(), &entry.value) {
            ("SGNM", FieldValue::String(value)) => CanonicalSubgraphField::BehaviorGraph(*value),
            ("SGNM", _) => CanonicalSubgraphField::InvalidBehaviorGraph,
            ("SAPT", FieldValue::String(value)) => CanonicalSubgraphField::Path(*value),
            ("SAKD", FieldValue::FormKey(value)) => CanonicalSubgraphField::SubgraphKeyword(*value),
            ("STKD", FieldValue::FormKey(value)) => CanonicalSubgraphField::TargetKeyword(*value),
            ("SRAF", FieldValue::Bytes(value)) => CanonicalSubgraphField::Flags(value.clone()),
            _ => return None,
        };
        Some(value)
    });

    parse_plain_canonical_subgraphs(fields)
        .into_iter()
        .map(|block| SubgraphBlock {
            behaviour_graph: block.behavior_graph,
            paths: block.paths,
            subgraph_keywords: block.subgraph_keywords,
            target_keywords: block.target_keywords,
            flags_bytes: block.flags,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// strip_template_subgraph_fields
// ---------------------------------------------------------------------------

/// Return a cloned `Record` with subgraph-data fields (SGNM/SAPT/SAKD/STKD/SRAF)
/// and the existing `SADD` (`SubgraphAdditiveRace`) stripped from the
/// `fields` list.
pub fn strip_template_subgraph_fields(record: &Record) -> Record {
    let drop_sigs: [Option<SubrecordSig>; 6] = [
        SubrecordSig::from_str("SGNM").ok(),
        SubrecordSig::from_str("SAPT").ok(),
        SubrecordSig::from_str("SAKD").ok(),
        SubrecordSig::from_str("STKD").ok(),
        SubrecordSig::from_str("SRAF").ok(),
        SubrecordSig::from_str("SADD").ok(),
    ];

    let mut clean = record.clone();
    let mut new_fields: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
    for entry in clean.fields.drain(..) {
        if drop_sigs.iter().any(|s| *s == Some(entry.sig)) {
            continue;
        }
        new_fields.push(entry);
    }
    clean.fields = new_fields;
    clean
}

// ---------------------------------------------------------------------------
// build_additive_race_record
// ---------------------------------------------------------------------------

/// Compose a canonical-shape additive RACE record. Starts from a stripped
/// template, appends one `SADD` reference, then appends the serialized
/// subgraph blocks in order.
///
/// The `template` is consumed (taken by value) — callers typically pass
/// `strip_template_subgraph_fields(&source)` or similar.
pub fn build_additive_race_record(
    template: Record,
    target_base_fk: FormKey,
    blocks: &[SubgraphBlock],
) -> Record {
    let mut out = template;
    let sadd = SubrecordSig::from_str("SADD").expect("SADD sig");
    out.fields.push(FieldEntry {
        sig: sadd,
        value: FieldValue::FormKey(target_base_fk),
    });
    for block in blocks {
        for entry in block_to_entries(block) {
            out.fields.push(entry);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Serialize one block back into a flat sequence of canonical `FieldEntry`s.
/// Order matches inspected FO4 records: SAKD*, STKD*, SGNM, SAPT*, SRAF.
fn block_to_entries(block: &SubgraphBlock) -> Vec<FieldEntry> {
    let sgnm = SubrecordSig::from_str("SGNM").expect("SGNM sig");
    let sapt = SubrecordSig::from_str("SAPT").expect("SAPT sig");
    let sakd = SubrecordSig::from_str("SAKD").expect("SAKD sig");
    let stkd = SubrecordSig::from_str("STKD").expect("STKD sig");
    let sraf = SubrecordSig::from_str("SRAF").expect("SRAF sig");

    let mut out: Vec<FieldEntry> = Vec::new();
    for kw in &block.subgraph_keywords {
        out.push(FieldEntry {
            sig: sakd,
            value: FieldValue::FormKey(*kw),
        });
    }
    for kw in &block.target_keywords {
        out.push(FieldEntry {
            sig: stkd,
            value: FieldValue::FormKey(*kw),
        });
    }
    out.push(FieldEntry {
        sig: sgnm,
        value: FieldValue::String(block.behaviour_graph),
    });
    for p in &block.paths {
        out.push(FieldEntry {
            sig: sapt,
            value: FieldValue::String(*p),
        });
    }
    if let Some(bytes) = &block.flags_bytes {
        out.push(FieldEntry {
            sig: sraf,
            value: FieldValue::Bytes(bytes.clone()),
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SigCode;
    use crate::record::{FieldEntry, Record, RecordFlags};
    use crate::sym::StringInterner;

    fn fk(local: u32, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    fn sym(s: &str, interner: &StringInterner) -> Sym {
        interner.intern(s)
    }

    fn entry_str(sig: &str, val: &str, interner: &StringInterner) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::String(sym(val, interner)),
        }
    }

    fn entry_fk(sig: &str, fk_val: FormKey) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::FormKey(fk_val),
        }
    }

    fn entry_bytes(sig: &str, bytes: &[u8]) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::Bytes(bytes.iter().copied().collect()),
        }
    }

    fn make_race(
        local: u32,
        plugin: &str,
        eid: &str,
        fields: Vec<FieldEntry>,
        interner: &StringInterner,
    ) -> Record {
        Record {
            sig: SigCode::from_str("RACE").unwrap(),
            form_key: fk(local, plugin, interner),
            eid: Some(sym(eid, interner)),
            flags: RecordFlags::empty(),
            fields: fields.into_iter().collect(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    /// empty record yields zero blocks.
    #[test]
    fn parse_empty_record_zero_blocks() {
        let mut interner = StringInterner::new();
        let r = make_race(0x800, "Output.esp", "HumanRace", vec![], &mut interner);
        assert!(parse_canonical_subgraphs(&r).is_empty());
    }

    /// SAPT entries before any SGNM are ignored.
    #[test]
    fn parse_orphan_sapt_ignored() {
        let mut interner = StringInterner::new();
        let fields = vec![entry_str("SAPT", "Actors\\stray", &mut interner)];
        let r = make_race(0x800, "Output.esp", "HumanRace", fields, &mut interner);
        assert!(parse_canonical_subgraphs(&r).is_empty());
    }

    /// single SGNM opens one block; trailing children attach.
    #[test]
    fn parse_single_block_with_children() {
        let mut interner = StringInterner::new();
        let kywd_a = fk(0x100, "Fallout4.esm", &mut interner);
        let kywd_b = fk(0x200, "Fallout4.esm", &mut interner);
        let fields = vec![
            entry_str("SGNM", "Actors\\Character\\HumanRace.hkx", &mut interner),
            entry_str(
                "SAPT",
                "Actors\\Character\\animations\\weapon\\Foo",
                &mut interner,
            ),
            entry_fk("SAKD", kywd_a),
            entry_fk("STKD", kywd_b),
            entry_bytes("SRAF", &[1, 0, 2, 0]),
        ];
        let r = make_race(0x800, "Output.esp", "HumanRace", fields, &mut interner);
        let blocks = parse_canonical_subgraphs(&r);
        assert_eq!(blocks.len(), 1);
        let b = &blocks[0];
        assert_eq!(
            interner.resolve(b.behaviour_graph).unwrap(),
            "Actors\\Character\\HumanRace.hkx"
        );
        assert_eq!(b.paths.len(), 1);
        assert_eq!(b.subgraph_keywords, vec![kywd_a]);
        assert_eq!(b.target_keywords, vec![kywd_b]);
        assert_eq!(b.flags_bytes.as_ref().unwrap().as_slice(), &[1u8, 0, 2, 0]);
    }

    /// each SGNM opens a new block; children attach to current.
    #[test]
    fn parse_multiple_blocks() {
        let mut interner = StringInterner::new();
        let kw1 = fk(0x100, "Fallout4.esm", &mut interner);
        let kw2 = fk(0x200, "Fallout4.esm", &mut interner);
        let fields = vec![
            entry_str("SGNM", "A.hkx", &mut interner),
            entry_fk("STKD", kw1),
            entry_str("SGNM", "B.hkx", &mut interner),
            entry_fk("STKD", kw2),
        ];
        let r = make_race(0x800, "Output.esp", "HumanRace", fields, &mut interner);
        let blocks = parse_canonical_subgraphs(&r);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].target_keywords, vec![kw1]);
        assert_eq!(blocks[1].target_keywords, vec![kw2]);
    }

    /// keyword rows before SGNM attach to the next block.
    #[test]
    fn parse_leading_keywords_attach_to_next_graph() {
        let mut interner = StringInterner::new();
        let kw_a = fk(0x100, "Fallout4.esm", &mut interner);
        let kw_b = fk(0x200, "Fallout4.esm", &mut interner);
        let fields = vec![
            entry_fk("SAKD", kw_a),
            entry_fk("STKD", kw_b),
            entry_str("SGNM", "A.hkx", &mut interner),
            entry_str("SAPT", "Actors\\A", &mut interner),
            entry_bytes("SRAF", &[1, 0, 0, 0]),
        ];
        let r = make_race(0x800, "Output.esp", "HumanRace", fields, &mut interner);
        let blocks = parse_canonical_subgraphs(&r);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].subgraph_keywords, vec![kw_a]);
        assert_eq!(blocks[0].target_keywords, vec![kw_b]);
    }

    /// after SRAF, keyword rows belong to the following graph.
    #[test]
    fn parse_keywords_after_sraf_attach_to_next_graph() {
        let mut interner = StringInterner::new();
        let kw_a = fk(0x100, "Fallout4.esm", &mut interner);
        let kw_b = fk(0x200, "Fallout4.esm", &mut interner);
        let fields = vec![
            entry_str("SGNM", "A.hkx", &mut interner),
            entry_str("SAPT", "Actors\\A", &mut interner),
            entry_bytes("SRAF", &[1, 0, 0, 0]),
            entry_fk("STKD", kw_a),
            entry_str("SGNM", "B.hkx", &mut interner),
            entry_fk("STKD", kw_b),
        ];
        let r = make_race(0x800, "Output.esp", "HumanRace", fields, &mut interner);
        let blocks = parse_canonical_subgraphs(&r);
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].target_keywords.is_empty());
        assert_eq!(blocks[1].target_keywords, vec![kw_a, kw_b]);
    }

    /// strip_template removes SGNM/SAPT/SAKD/STKD/SRAF/SADD but
    /// keeps other fields.
    #[test]
    fn strip_template_removes_subgraph_and_sadd() {
        let mut interner = StringInterner::new();
        let kw = fk(0x100, "Fallout4.esm", &mut interner);
        let parent = fk(0x166729, "Fallout4.esm", &mut interner);
        let fields = vec![
            entry_str("EDID", "HumanRace", &mut interner),
            entry_str("SGNM", "A.hkx", &mut interner),
            entry_str("SAPT", "P", &mut interner),
            entry_fk("SAKD", kw),
            entry_fk("STKD", kw),
            entry_bytes("SRAF", &[0u8; 4]),
            entry_fk("SADD", parent),
            entry_str("FULL", "Human Race", &mut interner),
        ];
        let r = make_race(0x800, "Output.esp", "HumanRace", fields, &mut interner);
        let stripped = strip_template_subgraph_fields(&r);
        let sigs: Vec<&str> = stripped.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"EDID"));
        assert!(sigs.contains(&"FULL"));
        for stripped_sig in &["SGNM", "SAPT", "SAKD", "STKD", "SRAF", "SADD"] {
            assert!(
                !sigs.contains(stripped_sig),
                "subrec {stripped_sig} should be stripped",
            );
        }
    }

    /// build then re-parse blocks equals the input (round-trip).
    #[test]
    fn build_then_parse_round_trip() {
        let mut interner = StringInterner::new();
        let parent = fk(0x166729, "Fallout4.esm", &mut interner);
        let kw_a = fk(0x100, "Fallout4.esm", &mut interner);
        let kw_b = fk(0x200, "Fallout4.esm", &mut interner);
        let block = SubgraphBlock {
            behaviour_graph: sym("Actors\\Character\\A.hkx", &mut interner),
            paths: vec![sym(
                "Actors\\Character\\animations\\weapon\\Foo",
                &mut interner,
            )],
            subgraph_keywords: vec![kw_a],
            target_keywords: vec![kw_b],
            flags_bytes: Some(smallvec::smallvec![5u8, 0, 0, 0]),
        };
        let template = make_race(0x800, "Output.esp", "HumanRace", vec![], &mut interner);
        let built = build_additive_race_record(template, parent, &[block.clone()]);

        // Exactly one SADD must be present.
        let sadd_count = built
            .fields
            .iter()
            .filter(|f| f.sig.as_str() == "SADD")
            .count();
        assert_eq!(sadd_count, 1);
        let sigs: Vec<&str> = built.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(&sigs[1..], &["SAKD", "STKD", "SGNM", "SAPT", "SRAF"]);

        let reparsed = parse_canonical_subgraphs(&built);
        assert_eq!(reparsed.len(), 1);
        assert_eq!(reparsed[0], block);
    }

    /// build with empty blocks list still emits a single SADD.
    #[test]
    fn build_with_no_blocks_only_emits_sadd() {
        let mut interner = StringInterner::new();
        let parent = fk(0x166729, "Fallout4.esm", &mut interner);
        let template = make_race(0x800, "Output.esp", "HumanRace", vec![], &mut interner);
        let built = build_additive_race_record(template, parent, &[]);
        assert_eq!(built.fields.len(), 1);
        assert_eq!(built.fields[0].sig.as_str(), "SADD");
        assert_eq!(built.fields[0].value, FieldValue::FormKey(parent));
    }
}
