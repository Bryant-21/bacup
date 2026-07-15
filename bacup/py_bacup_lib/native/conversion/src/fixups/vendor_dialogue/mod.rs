//! FO76→FO4 vendor-dialogue enablement (post-copy).
//!
//! FO76/Skyrim vendors trade from the faction's Vendor flag alone; FO4 instead
//! requires a "Let's trade" dialogue topic that runs the vanilla
//! `VendorInfoScript` (`OnEnd` → `ShowBarterMenu`). That topic ships in a
//! companion plugin (`B21_VendorDialogue.esp`, master = the converted output)
//! gated on `GetInFaction(B21_VendorDialogueFaction)`. This pass puts that gate
//! faction and its members into the converted output so the companion can find
//! them:
//!   1. synthesize FACT `B21_VendorDialogueFaction`;
//!   2. enroll every NPC that belongs to a vendor faction (a FACT carrying
//!      `VENC`, the merchant container) into it.
//!
//! Runs as a free function (NOT a registry `Fixup`) AFTER `repair_placed_child_refs`
//! has finalized `FACT.VENC` — the merchant container is a placed-child REFR
//! re-inserted post-fixups, so `VENC` is only reliably present that late.

mod build;

use rustc_hash::FxHashSet;

use crate::fixups::{FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::Record;
use crate::session::PluginSession;

use build::{build_vendor_dialogue_faction, enroll_npc_in_faction, npc_faction_formkeys};

pub const VENDOR_DIALOGUE_FACTION_EDID: &str = "B21_VendorDialogueFaction";
const SYNTH_VENDOR_PLUGIN: &str = "__synth_vendor_dialogue__";

pub fn synthesize_vendor_dialogue(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
) -> Result<FixupReport, FixupError> {
    let mut report = FixupReport::empty();
    let target_schema = config
        .target_schema
        .as_deref()
        .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
    let fact_sig = SigCode::from_str("FACT").map_err(FixupError::SchemaError)?;
    let npc_sig = SigCode::from_str("NPC_").map_err(FixupError::SchemaError)?;

    // 1. Vendor factions = output FACTs carrying VENC (the merchant container).
    let mut vendor_factions: FxHashSet<FormKey> = FxHashSet::default();
    for fk in session
        .form_keys_of_sig(fact_sig, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?
    {
        if session
            .record_has_any_subrecord(&fk, &["VENC"])
            .unwrap_or(false)
        {
            vendor_factions.insert(fk);
        }
    }
    if vendor_factions.is_empty() {
        return Ok(report);
    }

    // 2. Synthesize the gate faction.
    let synth_source = FormKey {
        local: 0,
        plugin: mapper.interner.intern(SYNTH_VENDOR_PLUGIN),
    };
    let fact_fk = mapper.allocate_or_resolve(synth_source, None, fact_sig);
    let fact =
        build_vendor_dialogue_faction(fact_fk, VENDOR_DIALOGUE_FACTION_EDID, mapper.interner);
    session
        .add_record(fact, target_schema, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    report.records_added += 1;

    // 3. Enroll every NPC that belongs to a vendor faction.
    let mut changed: Vec<Record> = Vec::new();
    for fk in session
        .form_keys_of_sig(npc_sig, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?
    {
        if !session
            .record_has_any_subrecord(&fk, &["SNAM"])
            .unwrap_or(false)
        {
            continue;
        }
        let mut rec = match session.record_decoded(&fk, target_schema, mapper.interner) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let is_vendor = npc_faction_formkeys(&rec)
            .iter()
            .any(|f| vendor_factions.contains(f));
        if is_vendor && enroll_npc_in_faction(&mut rec, fact_fk) {
            changed.push(rec);
        }
    }
    let expected = changed.len();
    if expected > 0 {
        let replaced = session
            .replace_records_contents(changed, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        report.records_changed = replaced.try_into().unwrap_or(u32::MAX);
    }

    eprintln!(
        "[vendor_dialogue] vendor_factions={} gate_fact={:06X}@{} enrolled_npcs={}",
        vendor_factions.len(),
        fact_fk.local & 0x00FF_FFFF,
        mapper.interner.resolve(fact_fk.plugin).unwrap_or("?"),
        report.records_changed
    );
    Ok(report)
}

#[cfg(test)]
mod tests {
    // Record-builder + enrollment unit tests live in `build.rs`. The
    // `synthesize_vendor_dialogue` orchestration is exercised end-to-end by the
    // conversion baselines (it needs a live `PluginSession`/`FormKeyMapper`).
}
