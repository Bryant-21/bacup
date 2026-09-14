//! Legacy `ACRE` placed-creature lowering for FNV/FO3 → FO4.
//!
//! An `ACRE` carries the same placed-reference payload as an `ACHR`, but its
//! `NAME` points at a legacy `CREA` base.  The caller must lower the signature
//! before schema translation, then resolve the base after the converted `NPC_`
//! base has registered its FormKey mapping.  Keeping those two operations
//! explicit prevents an ACRE from being emitted with an unmapped legacy base.

use rustc_hash::FxHashMap;

use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

const ACRE_SIG: SigCode = SigCode(*b"ACRE");
const ACHR_SIG: SigCode = SigCode(*b"ACHR");
const NAME_SIG: SubrecordSig = SubrecordSig(*b"NAME");

/// Why a legacy creature placement was not eligible for an FO4 `ACHR`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AcreLoweringError {
    UnexpectedSignature(SigCode),
    MissingBase {
        placed: FormKey,
    },
    UnmappedBase {
        placed: FormKey,
        base: FormKey,
    },
    MappingConflict {
        placed: FormKey,
        existing: FormKey,
        requested: FormKey,
    },
}

impl std::fmt::Display for AcreLoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedSignature(sig) => write!(f, "expected ACRE, found {}", sig.as_str()),
            Self::MissingBase { placed } => {
                write!(f, "ACRE {:06X} has no decodable NAME base", placed.local)
            }
            Self::UnmappedBase { placed, base } => write!(
                f,
                "ACRE {:06X} base {:06X} has no converted NPC_ mapping",
                placed.local, base.local
            ),
            Self::MappingConflict {
                placed,
                existing,
                requested,
            } => write!(
                f,
                "ACRE {:06X} target mapping conflict: {:06X} vs {:06X}",
                placed.local, existing.local, requested.local
            ),
        }
    }
}

impl std::error::Error for AcreLoweringError {}

/// A direct placed-actor target available to a converted quest alias.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlacedActorAliasTarget {
    pub source_placed: FormKey,
    pub target_placed: FormKey,
    pub source_base: FormKey,
    pub target_base: FormKey,
}

/// Registry populated only after a placed actor has an emitted target FormKey.
///
/// Quest alias lowering can depend on this narrow contract instead of guessing
/// that every FormKey mapper entry denotes a placed actor.
#[derive(Debug, Default)]
pub(crate) struct PlacedActorAliasResolver {
    targets: FxHashMap<FormKey, PlacedActorAliasTarget>,
}

impl PlacedActorAliasResolver {
    pub(crate) fn register(
        &mut self,
        target: PlacedActorAliasTarget,
    ) -> Result<(), AcreLoweringError> {
        if let Some(existing) = self.targets.get(&target.source_placed)
            && existing.target_placed != target.target_placed
        {
            return Err(AcreLoweringError::MappingConflict {
                placed: target.source_placed,
                existing: existing.target_placed,
                requested: target.target_placed,
            });
        }
        self.targets.insert(target.source_placed, target);
        Ok(())
    }

    /// Resolve a source ACRE/ACHR direct target for a quest alias.
    pub(crate) fn resolve_direct(&self, source_placed: FormKey) -> Option<PlacedActorAliasTarget> {
        self.targets.get(&source_placed).copied()
    }
}

/// Change a source `ACRE` signature to `ACHR` before schema translation.
///
/// This preserves record flags and placed fields, including `DATA`; the generic
/// target mapper owns local FormKey allocation and production cell grouping.
pub(crate) fn lower_acre_signature(record: &mut Record) -> Result<(), AcreLoweringError> {
    if record.sig != ACRE_SIG {
        return Err(AcreLoweringError::UnexpectedSignature(record.sig));
    }
    record.sig = ACHR_SIG;
    Ok(())
}

/// Resolve the mapped NPC_ base on an already translated `ACHR`.
///
/// The caller invokes this immediately before writing the record, after its
/// own target FormKey has been allocated.  It fails closed when the legacy
/// creature base did not produce an NPC_ target, and registers the direct
/// placed target for later quest-alias lowering.
pub(crate) fn finalize_lowered_acre(
    record: &mut Record,
    source_placed: FormKey,
    target_placed: FormKey,
    mapper: &mut FormKeyMapper<'_>,
    aliases: &mut PlacedActorAliasResolver,
    interner: &StringInterner,
) -> Result<PlacedActorAliasTarget, AcreLoweringError> {
    if record.sig != ACHR_SIG {
        return Err(AcreLoweringError::UnexpectedSignature(record.sig));
    }
    let source_base =
        placed_actor_base_formkey(record, interner).ok_or(AcreLoweringError::MissingBase {
            placed: source_placed,
        })?;
    let target_base = mapper
        .lookup(source_base)
        .ok_or(AcreLoweringError::UnmappedBase {
            placed: source_placed,
            base: source_base,
        })?;
    set_record_base_formkey(record, target_base, interner)
        .expect("record_base_formkey located a mutable NAME field");

    if let Some(existing) = mapper.lookup(source_placed) {
        if existing != target_placed {
            return Err(AcreLoweringError::MappingConflict {
                placed: source_placed,
                existing,
                requested: target_placed,
            });
        }
    } else {
        mapper.add_mapping(source_placed, target_placed);
    }

    let target = PlacedActorAliasTarget {
        source_placed,
        target_placed,
        source_base,
        target_base,
    };
    aliases.register(target)?;
    Ok(target)
}

/// Read the direct `NAME` base from a translated placed actor.
pub(crate) fn placed_actor_base_formkey(
    record: &Record,
    interner: &StringInterner,
) -> Option<FormKey> {
    record
        .fields
        .iter()
        .find(|field| field.sig == NAME_SIG)
        .and_then(|field| formkey_from_value(&field.value, interner))
}

fn set_record_base_formkey(
    record: &mut Record,
    target_base: FormKey,
    interner: &StringInterner,
) -> Option<()> {
    let field = record
        .fields
        .iter_mut()
        .find(|field| field.sig == NAME_SIG)?;
    set_formkey_in_value(&mut field.value, target_base, interner)
}

fn formkey_from_value(value: &FieldValue, interner: &StringInterner) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(form_key) => Some(*form_key),
        FieldValue::Struct(fields) => fields
            .iter()
            .find(|(key, _)| interner.resolve(*key) == Some("Base"))
            .and_then(|(_, value)| formkey_from_value(value, interner)),
        _ => None,
    }
}

fn set_formkey_in_value(
    value: &mut FieldValue,
    target_base: FormKey,
    interner: &StringInterner,
) -> Option<()> {
    match value {
        FieldValue::FormKey(form_key) => {
            *form_key = target_base;
            Some(())
        }
        FieldValue::Struct(fields) => fields
            .iter_mut()
            .find(|(key, _)| interner.resolve(*key) == Some("Base"))
            .and_then(|(_, value)| set_formkey_in_value(value, target_base, interner)),
        _ => None,
    }
}
