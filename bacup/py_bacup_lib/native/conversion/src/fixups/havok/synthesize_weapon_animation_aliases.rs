//! Supply FO4 shared-behavior clip names for converted FO76 animations.
//!
//! FO76's third-person `GunBehavior` uses underscored additive names and omits
//! the non-slave ready clips that FO4's shared `WeaponBehavior` requests. When
//! a converted RACE is redirected to `WeaponBehavior`, the missing bindings
//! make the actor T-pose only while firing.
//!
//! FO4's shared `MTBehavior` similarly requests lean-named locomotion clips
//! where FO76 uses left/right names. Without aliases, actors move without
//! playing a lower-body animation.
//!
//! FO4's shared `MeleeBehavior` also exposes backward, diagonal, and sprinting
//! states that are absent from FO76 creature H2H folders.

use std::path::{Component, Path, PathBuf};

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::{AssetPhaseFlags, FixupScope};
use crate::session::PluginSession;

const WEAPON_ANIMATION_ALIASES: &[(&str, &str)] = &[
    // FO76's `wpnfire*_additive.hkx` is FO4's `wpnfire*ready.hkx` in all but name: a
    // full-body clip carrying the `weaponFire` events (GripHeavy: FO4 `wpnfireautoready`
    // 95 tracks / 20 events, FO76 `wpnfireauto_additive` 96 / 20 at identical times, FO4
    // `wpnfireautoadditive` a 13-track delta with none). The suffix names how FO76's graph
    // blends the clip, not what it holds, so it must not map onto FO4's additive slot.
    //
    // Ordered ahead of the `*ReadySlave` entries below so the annotated full-body clip
    // wins wherever FO76 shipped one, and ahead of the sneak/left entries so those can
    // chain off the copy it makes.
    ("wpnfireauto_additive.hkx", "wpnfireautoready.hkx"),
    ("wpnfiresingle_additive.hkx", "wpnfiresingleready.hkx"),
    // The slave is only a valid master where it actually carries the events — FO76's Gauss
    // slave does, most do not. `synthesize_aliases_in_tree` gates on the annotation
    // per-file rather than per-name.
    ("wpnfireautoreadyslave.hkx", "wpnfireautoready.hkx"),
    ("wpnfiresinglereadyslave.hkx", "wpnfiresingleready.hkx"),
    // Gun-stance clip names FO4's shared WeaponBehavior requests that FO76 gun creatures
    // never authored (MoleMiner GripAssault census vs the FO4 graph). The base
    // sneak aliases come first so the sneak-derived entries further down can chain off the
    // copies created in the same pass.
    ("wpnidleready.hkx", "sneakwpnidleready.hkx"),
    ("wpnwalkforwardready.hkx", "sneakwpnwalkforwardready.hkx"),
    ("wpnwalkbackwardready.hkx", "sneakwpnwalkbackwardready.hkx"),
    ("wpnwalkleftready.hkx", "sneakwpnwalkleftready.hkx"),
    ("wpnwalkrightready.hkx", "sneakwpnwalkrightready.hkx"),
    ("wpnrunforwardready.hkx", "sneakwpnrunforwardready.hkx"),
    ("wpnrunbackpedalready.hkx", "sneakwpnrunbackpedalready.hkx"),
    ("wpnrunleftready.hkx", "sneakwpnrunleftready.hkx"),
    ("wpnrunrightready.hkx", "sneakwpnrunrightready.hkx"),
    ("wpnfireautoready.hkx", "sneakwpnfireautoready.hkx"),
    ("wpnfiresingleready.hkx", "sneakwpnfiresingleready.hkx"),
    ("turninplaceleft90.hkx", "sneakturninplaceleft90.hkx"),
    ("turninplaceright90.hkx", "sneakturninplaceright90.hkx"),
    ("turninplaceleft180.hkx", "sneakturninplaceleft180.hkx"),
    ("turninplaceright180.hkx", "sneakturninplaceright180.hkx"),
    ("wpnidleready.hkx", "sneakidletorifletrans.hkx"),
    ("wpnidleready.hkx", "sneakrifletoidletrans.hkx"),
    ("wpnidleready.hkx", "sneakwpnentercover.hkx"),
    // Combat shuffle-steps (range adjustment) from the matching walk direction.
    ("wpnwalkforwardready.hkx", "wpnstepforwardready.hkx"),
    ("wpnwalkforwardready.hkx", "wpnstepforwardready_var1.hkx"),
    ("wpnwalkforwardready.hkx", "wpnstepforwardleftready.hkx"),
    ("wpnwalkforwardready.hkx", "wpnstepforwardrightready.hkx"),
    (
        "wpnwalkforwardready.hkx",
        "wpnstepforwardrightready_var1.hkx",
    ),
    ("wpnwalkbackwardready.hkx", "wpnstepbackwardready.hkx"),
    ("wpnwalkbackwardready.hkx", "wpnstepbackwardready_var1.hkx"),
    ("wpnwalkbackwardready.hkx", "wpnstepbackwardleftready.hkx"),
    ("wpnwalkbackwardready.hkx", "wpnstepbackwardrightready.hkx"),
    ("wpnwalkleftready.hkx", "wpnstepleftready.hkx"),
    ("wpnwalkrightready.hkx", "wpnsteprightready.hkx"),
    ("wpnwalkrightready.hkx", "wpnsteprightready_var1.hkx"),
    // Jumps and ledge falls while a weapon is drawn.
    ("wpnrunforwardready.hkx", "wpnjumprun.hkx"),
    ("wpnidleready.hkx", "wpnjumprunfall.hkx"),
    ("wpnidleready.hkx", "wpnjumprunland.hkx"),
    ("wpnidleready.hkx", "wpnjumprunimpactland.hkx"),
    ("wpnidleready.hkx", "wpnjumpimpactland.hkx"),
    ("wpnidleready.hkx", "wpnjumpinplace.hkx"),
    ("wpnidleready.hkx", "wpnjumpinplacefall.hkx"),
    ("wpnidleready.hkx", "wpnjumpinplaceland.hkx"),
    // Knockdown recovery, essential-down, crits, bare idle.
    ("wpnidleready.hkx", "getup_face_down.hkx"),
    ("wpnidleready.hkx", "getup_face_up.hkx"),
    ("wpnidleready.hkx", "essentialdown.hkx"),
    ("wpnidleready.hkx", "essentialdownexit.hkx"),
    ("wpnidleready.hkx", "critbothlegsfalldown.hkx"),
    ("wpnidleready.hkx", "riflecritbothlegsfalldown.hkx"),
    ("wpnidleready.hkx", "idle.hkx"),
    // Hit stumbles and directional hit reactions.
    ("staggerforwardmedium.hkx", "raiderstumbleforward2step.hkx"),
    (
        "staggerforwardmedium.hkx",
        "raiderstumbleforwardleft2step.hkx",
    ),
    (
        "staggerforwardmedium.hkx",
        "raiderstumbleforwardright2step.hkx",
    ),
    ("staggerbackmedium.hkx", "raiderstumblebackward2step.hkx"),
    ("staggerbackmedium.hkx", "raiderstumblebackleft2step.hkx"),
    ("staggerbackmedium.hkx", "raiderstumblebackright2step.hkx"),
    ("staggerbackmedium.hkx", "raiderstumbleleft2step.hkx"),
    ("staggerbackmedium.hkx", "raiderstumbleright2step.hkx"),
    ("hitchestfront.hkx", "riflehitleftchest1.hkx"),
    ("hitchestfront.hkx", "riflehitrightchest1.hkx"),
    ("hitheadfront.hkx", "riflehitlefthead1.hkx"),
    ("hitheadfront.hkx", "riflehitrighthead1.hkx"),
    ("hitheadfront.hkx", "riflehitbackhead1.hkx"),
    ("hitleftarmfront.hkx", "riflehitbacklarm1.hkx"),
    ("hitrightarmfront.hkx", "riflehitbackrarm1.hkx"),
    ("hitheadfront.hkx", "riflevatscrit_head_front_01.hkx"),
    ("hitleftarmfront.hkx", "riflevatscrit_leftarm_front_01.hkx"),
    (
        "hitrightarmfront.hkx",
        "riflevatscrit_rightarm_front_01.hkx",
    ),
    // Charge, reload loop, panic fire, throwables, gun-melee, assembly, dodge variants.
    ("wpnboltchargeready.hkx", "wpnchargeup.hkx"),
    ("wpnboltchargeready.hkx", "wpnchargedown.hkx"),
    ("wpnreload.hkx", "wpnreloadloop.hkx"),
    ("wpnfireautoready.hkx", "riflepanicfire.hkx"),
    ("wpngrenadethrow.hkx", "wpnminethrow.hkx"),
    (
        "wpngrenadethrow.hkx",
        "riflecoverstandingsightedgrenadethrow.hkx",
    ),
    (
        "wpngrenadethrow.hkx",
        "riflecoverstandingsightedgrenadethrowleft.hkx",
    ),
    (
        "wpngrenadethrow.hkx",
        "rifleidlereadykneelsightedgrenadethrow.hkx",
    ),
    (
        "wpngrenadethrow.hkx",
        "rifleidlereadykneelsightedgrenadethrowleft.hkx",
    ),
    (
        "wpngrenadethrow.hkx",
        "rifleidlereadykneelsightedgrenadethrowoverleft.hkx",
    ),
    (
        "wpngrenadethrow.hkx",
        "sneakrifleidlereadykneelsightedgrenadethrowoverleft.hkx",
    ),
    ("riflemeleeforwardpowerattack.hkx", "wpnmeleeshredder.hkx"),
    ("wpnassemblypose.hkx", "wpnassemblypose_left.hkx"),
    ("wpndodgeleft.hkx", "wpndodgeleft_var1.hkx"),
    ("wpndodgeavoidleft.hkx", "wpndodgeavoidleft_var1.hkx"),
    ("wpndodgeavoidright.hkx", "wpndodgeavoidright_var1.hkx"),
    ("posea_idleflavor1.hkx", "rifleidlealtguncheck_01.hkx"),
    // Relaxed turn loops.
    (
        "turninplaceleft180_loop.hkx",
        "relaxedturninplaceleft180_loop.hkx",
    ),
    (
        "turninplaceright180_loop.hkx",
        "relaxedturninplaceright180_loop.hkx",
    ),
    (
        "turninplaceleft60_loop.hkx",
        "relaxedturninplaceleft60_loop.hkx",
    ),
    (
        "turninplaceright60_loop.hkx",
        "relaxedturninplaceright60_loop.hkx",
    ),
    // Crouched/kneeling cover and vaults map onto the standing-cover set FO76 authored.
    (
        "riflecoverstandingidle.hkx",
        "rifleidlereadycoverrightkneel.hkx",
    ),
    (
        "riflecoverstandingidle.hkx",
        "rifleidlereadycoverrightkneelshuffleforward.hkx",
    ),
    (
        "riflecoverstandingidle.hkx",
        "rifleidlereadycovermirrortrans.hkx",
    ),
    (
        "riflecoverstandingidle.hkx",
        "riflecoverstandingidlerighttolefttrans.hkx",
    ),
    (
        "riflecoverstandingidle.hkx",
        "riflecrouchcovertostandingcover.hkx",
    ),
    (
        "riflecoverstandingidle.hkx",
        "riflestandingcovertocrouchcover.hkx",
    ),
    ("riflecoverstandingidle.hkx", "riflesprintentercover.hkx"),
    (
        "riflecoverstandingidlesighted.hkx",
        "rifleidlereadykneelrightsighted.hkx",
    ),
    (
        "riflecoverstandingidlesighted.hkx",
        "rifleidlereadykneelrightsightedshuffleforward.hkx",
    ),
    (
        "riflecoverstandingidlesightedtrans.hkx",
        "rifleidlereadykneelrightsightedtrans.hkx",
    ),
    (
        "riflecoverstandingidlesightedtransrev.hkx",
        "rifleidlereadykneelrightsightedtransrev.hkx",
    ),
    (
        "riflecoverstandingsightedfireauto.hkx",
        "rifleidlereadykneelrightsightedfireauto.hkx",
    ),
    (
        "riflecoverstandingsightedfireauto.hkx",
        "rifleidlereadykneelrightblindfireoverauto.hkx",
    ),
    (
        "riflecoverstandingsightedfiresingle.hkx",
        "rifleidlereadykneelrightsightedfiresingle.hkx",
    ),
    (
        "riflecoverstandingsightedfiresingle.hkx",
        "rifleidlereadykneelrightblindfireoversingle.hkx",
    ),
    (
        "riflecoverstandingrightfireautoblind.hkx",
        "rifleidlereadykneelrightblindfireauto.hkx",
    ),
    (
        "riflecoverstandingrightfireautoblind.hkx",
        "rifleidlereadykneelrightblindfireautoshuffleforward.hkx",
    ),
    (
        "riflecoverstandingrightfiresingleblind.hkx",
        "rifleidlereadykneelrightblindfiresingle.hkx",
    ),
    (
        "riflecoverstandingrightfireautoblindtrans.hkx",
        "rifleidlereadykneelrightblindfireautotrans.hkx",
    ),
    (
        "riflecoverstandingrightfireautoblindtransrev.hkx",
        "rifleidlereadykneelrightblindfireautotransrev.hkx",
    ),
    (
        "riflecoverstandingrightfireautoblindtrans.hkx",
        "rifleidlereadykneelrightblindovertrans.hkx",
    ),
    (
        "riflecoverstandingrightfireautoblindtransrev.hkx",
        "rifleidlereadykneelrightblindovertransrev.hkx",
    ),
    (
        "riflecoverstandingrightentertrans.hkx",
        "rifleidlereadykneelrightentertranscutcycle.hkx",
    ),
    (
        "riflecoverstandingrightexittrans.hkx",
        "rifleidlereadykneelrightexittrans.hkx",
    ),
    (
        "riflecoverstandingidlesightedtrans.hkx",
        "rifleidlereadykneelrightoversightedtrans.hkx",
    ),
    (
        "riflecoverstandingidlesightedtransrev.hkx",
        "rifleidlereadykneelrightoversightedtransrev.hkx",
    ),
    (
        "riflecoverstandingidlesightedtrans.hkx",
        "rifleidlereadykneelrightoversightedlowtrans.hkx",
    ),
    (
        "riflecoverstandingidlesightedtransrev.hkx",
        "rifleidlereadykneelrightoversightedlowtransrev.hkx",
    ),
    (
        "riflecoverstandingrightfireautoblind.hkx",
        "riflecoverstandingrightfireautoblindshuffleforward.hkx",
    ),
    (
        "walkbackcoverright.hkx",
        "riflecrouchwalkbackcoverright.hkx",
    ),
    (
        "walkforwardcoverright.hkx",
        "riflecrouchwalkforwardcoverright.hkx",
    ),
    (
        "walkrightcoverright.hkx",
        "riflecrouchwalkrightcoverright.hkx",
    ),
    (
        "riflereloadcoverstandingleft.hkx",
        "riflereloadcoverleftkneel.hkx",
    ),
    (
        "riflereloadcoverstanding.hkx",
        "riflereloadcoverrightkneel.hkx",
    ),
    ("wpnidleready.hkx", "rifleidlereadykneelrightvault48.hkx"),
    (
        "wpnidleready.hkx",
        "rifleidlereadykneelrightvault48forward20.hkx",
    ),
    ("wpnidleready.hkx", "rifleidlereadykneelrightvault64.hkx"),
    (
        "wpnidleready.hkx",
        "rifleidlereadykneelrightvault64forward20.hkx",
    ),
    (
        "wpnidleready.hkx",
        "rifleidlereadykneelrightvault64nonsneak.hkx",
    ),
    (
        "wpnidleready.hkx",
        "rifleidlereadykneelrightvault64nonsneakforward20.hkx",
    ),
    ("wpnidleready.hkx", "rifleidlereadykneelrightvault80.hkx"),
    (
        "wpnidleready.hkx",
        "rifleidlereadykneelrightvault80forward20.hkx",
    ),
    ("wpnfireautosighted.hkx", "wpnfireautosighted_left.hkx"),
    ("wpnfiresinglesighted.hkx", "wpnfiresinglesighted_left.hkx"),
    (
        "sneakwpnfireautoready.hkx",
        "sneakwpnfireautoready_left.hkx",
    ),
    (
        "sneakwpnfiresingleready.hkx",
        "sneakwpnfiresingleready_left.hkx",
    ),
    ("sneakwpnidleready.hkx", "sneakwpnidleready_left.hkx"),
    (
        "sneakwpnrunbackpedalready.hkx",
        "sneakwpnrunbackpedalleftready.hkx",
    ),
    (
        "sneakwpnrunbackpedalready.hkx",
        "sneakwpnrunbackpedalrightready.hkx",
    ),
    (
        "sneakwpnrunforwardready.hkx",
        "sneakwpnrunforwardleftready.hkx",
    ),
    (
        "sneakwpnrunforwardready.hkx",
        "sneakwpnrunforwardrightready.hkx",
    ),
    ("sneakwpnrunleftready.hkx", "sneakwpnrunleftready_back.hkx"),
    (
        "sneakwpnrunrightready.hkx",
        "sneakwpnrunrightready_back.hkx",
    ),
    (
        "sneakwpnwalkbackwardready.hkx",
        "sneakwpnwalkbackwardleftready.hkx",
    ),
    (
        "sneakwpnwalkbackwardready.hkx",
        "sneakwpnwalkbackwardrightready.hkx",
    ),
    (
        "sneakwpnwalkforwardready.hkx",
        "sneakwpnwalkforwardleftready.hkx",
    ),
    (
        "sneakwpnwalkforwardready.hkx",
        "sneakwpnwalkforwardrightready.hkx",
    ),
    (
        "sneakwpnwalkleftready.hkx",
        "sneakwpnwalkleftready_back.hkx",
    ),
    (
        "sneakwpnwalkrightready.hkx",
        "sneakwpnwalkrightready_back.hkx",
    ),
    ("wpnidleready.hkx", "wpnidlerelaxed_to_ready.hkx"),
    ("wpnidleready.hkx", "wpnidleready_to_relaxed.hkx"),
    ("wpnidleready.hkx", "wpnidlesighted.hkx"),
    ("wpnidleready.hkx", "wpnidlesighted_left.hkx"),
    ("wpnsightedadd.hkx", "wpnidlealertadd.hkx"),
    ("wpnrunbackpedalready.hkx", "wpnrunbackpedalleftready.hkx"),
    ("wpnrunbackpedalready.hkx", "wpnrunbackpedalleftrelaxed.hkx"),
    ("wpnrunbackpedalready.hkx", "wpnrunbackpedalrelaxed.hkx"),
    (
        "wpnrunbackpedalready.hkx",
        "wpnrunbackpedalrightready_forward.hkx",
    ),
    ("wpnrunbackpedalready.hkx", "wpnrunbackpedalrightready.hkx"),
    (
        "wpnrunbackpedalready.hkx",
        "wpnrunbackpedalrightrelaxed.hkx",
    ),
    ("wpnrunforwardready.hkx", "wpnrunforwardleftready_back.hkx"),
    ("wpnrunforwardready.hkx", "wpnrunforwardleftready.hkx"),
    ("wpnrunforwardready.hkx", "wpnrunforwardleftrelaxed.hkx"),
    ("wpnrunforwardready.hkx", "wpnrunforwardrelaxed.hkx"),
    ("wpnrunforwardready.hkx", "wpnrunforwardrightready.hkx"),
    ("wpnrunforwardready.hkx", "wpnrunforwardrightrelaxed.hkx"),
    ("wpnrunleftready.hkx", "wpnrunleftready_back.hkx"),
    ("wpnrunleftready.hkx", "wpnrunleftrelaxed_back.hkx"),
    ("wpnrunleftready.hkx", "wpnrunleftrelaxed.hkx"),
    ("wpnrunrightready.hkx", "wpnrunrightready_back.hkx"),
    ("wpnrunrightready.hkx", "wpnrunrightrelaxed_back.hkx"),
    ("wpnrunrightready.hkx", "wpnrunrightrelaxed.hkx"),
    ("wpnwalkbackwardready.hkx", "wpnwalkbackwardleftready.hkx"),
    ("wpnwalkbackwardready.hkx", "wpnwalkbackwardleftrelaxed.hkx"),
    ("wpnwalkbackwardready.hkx", "wpnwalkbackwardrelaxed.hkx"),
    (
        "wpnwalkbackwardready.hkx",
        "wpnwalkbackwardrightready_forward.hkx",
    ),
    ("wpnwalkbackwardready.hkx", "wpnwalkbackwardrightready.hkx"),
    (
        "wpnwalkbackwardready.hkx",
        "wpnwalkbackwardrightrelaxed.hkx",
    ),
    (
        "wpnwalkforwardready.hkx",
        "wpnwalkforwardleftready_back.hkx",
    ),
    ("wpnwalkforwardready.hkx", "wpnwalkforwardleftready.hkx"),
    ("wpnwalkforwardready.hkx", "wpnwalkforwardleftrelaxed.hkx"),
    ("wpnwalkforwardready.hkx", "wpnwalkforwardrelaxed.hkx"),
    ("wpnwalkforwardready.hkx", "wpnwalkforwardrightready.hkx"),
    ("wpnwalkforwardready.hkx", "wpnwalkforwardrightrelaxed.hkx"),
    ("wpnwalkleftready.hkx", "wpnwalkleftready_back.hkx"),
    ("wpnwalkleftready.hkx", "wpnwalkleftrelaxed_back.hkx"),
    ("wpnwalkleftready.hkx", "wpnwalkleftrelaxed.hkx"),
    ("wpnwalkrightready.hkx", "wpnwalkrightready_back.hkx"),
    ("wpnwalkrightready.hkx", "wpnwalkrightrelaxed_back.hkx"),
    ("wpnwalkrightready.hkx", "wpnwalkrightrelaxed.hkx"),
];

const MT_ANIMATION_ALIASES: &[(&str, &str)] = &[
    ("posea_idle1.hkx", "idle.hkx"),
    // A FO4 lean is the forward gait banked to one side, not a strafe: vanilla
    // `MT\Neutral\JogLeanRight` measures +Y at 225.48, identical to `JogForward`. FO76 ships no
    // walk/jog/run leans (only sprint ones, also +Y), so these six come from the forward clips.
    // The speed producer reads one leaf clip per gait, `*LeanRight` for the shared
    // `MTBehavior`; a ±X strafe source records the gait as sideways, leaves no forward row,
    // and the engine zeroes jog/run.
    ("jogforward.hkx", "jogleanleft.hkx"),
    ("jogforward.hkx", "jogleanright.hkx"),
    ("runforward.hkx", "runleanleft.hkx"),
    ("runforward.hkx", "runleanright.hkx"),
    ("walkforward.hkx", "walkleanleft.hkx"),
    ("walkforward.hkx", "walkleanright.hkx"),
    // 8-way blend diagonals and forward-facing corners (MoleMiner MT census vs the FO4
    // shared graphs).
    ("walkforward.hkx", "walkforwardleft.hkx"),
    ("walkforward.hkx", "walkforwardright.hkx"),
    ("walkbackward.hkx", "walkbackwardleft.hkx"),
    ("walkbackward.hkx", "walkbackwardright.hkx"),
    ("runforward.hkx", "runforwardleft.hkx"),
    ("runforward.hkx", "runforwardright.hkx"),
    ("runbackward.hkx", "runbackwardleft.hkx"),
    ("runbackward.hkx", "runbackwardright.hkx"),
    ("runleft.hkx", "runbackwardleftforwardfacing.hkx"),
    ("runright.hkx", "runbackwardrightforwardfacing.hkx"),
    // Jumps and ledge falls — without these a creature walking off any ledge T-poses.
    ("runforward.hkx", "jumprun.hkx"),
    ("walkforward.hkx", "jumpwalk.hkx"),
    ("posea_idle1.hkx", "jumprunfall.hkx"),
    ("posea_idle1.hkx", "jumprunland.hkx"),
    ("posea_idle1.hkx", "jumprunimpactland.hkx"),
    ("posea_idle1.hkx", "jumpwalkfall.hkx"),
    ("posea_idle1.hkx", "jumpwalkland.hkx"),
    ("posea_idle1.hkx", "jumpimpactland.hkx"),
    ("posea_idle1.hkx", "jumpinplace.hkx"),
    ("posea_idle1.hkx", "jumpinplacefall.hkx"),
    ("posea_idle1.hkx", "jumpinplaceland.hkx"),
    // Stagger blend members.
    ("staggerbackmedium.hkx", "staggerleftmedium.hkx"),
    ("staggerbackmedium.hkx", "staggerrightmedium.hkx"),
    ("staggerbackmedium.hkx", "staggerblended.hkx"),
    // Fear / cower reactions.
    ("posea_idle1.hkx", "enter_cower1.hkx"),
    ("posea_idle1.hkx", "enter_cower2.hkx"),
    ("posea_idle1.hkx", "enter_cower3.hkx"),
    ("posea_idle1.hkx", "enter_cower4.hkx"),
    ("posea_idle1.hkx", "exit_cower1.hkx"),
    ("posea_idle1.hkx", "exit_cower2.hkx"),
    ("posea_idle1.hkx", "exit_cower3.hkx"),
    ("posea_idle1.hkx", "exit_cower4.hkx"),
    ("posea_idle1.hkx", "posea_cower1.hkx"),
    ("posea_idle1.hkx", "posea_cower2.hkx"),
    ("posea_idle1.hkx", "posea_cower3.hkx"),
    ("posea_idle1.hkx", "posea_cower4.hkx"),
    // Sandbox eat/drink package idles.
    ("posea_idle1.hkx", "drinkstart.hkx"),
    ("posea_idle1.hkx", "drinkloop.hkx"),
    ("posea_idle1.hkx", "drinkend.hkx"),
    ("posea_idle1.hkx", "eatstart.hkx"),
    ("posea_idle1.hkx", "eatidle.hkx"),
    ("posea_idle1.hkx", "eatend.hkx"),
    // Sneak stance.
    ("posea_idle1.hkx", "sneakidle.hkx"),
    ("posea_idle1.hkx", "idletosneaktrans.hkx"),
    ("posea_idle1.hkx", "sneaktoidletrans.hkx"),
    ("posea_idle1.hkx", "sneakstand_to_sneakwalk.hkx"),
    ("posea_idle1.hkx", "sneakstand_to_sneakrun.hkx"),
    ("posea_idle1.hkx", "sneakrun_to_sneakstand.hkx"),
    ("walkforward.hkx", "sneakwalkforward.hkx"),
    ("runforward.hkx", "sneakrunforward.hkx"),
    ("walkforward.hkx", "sneakwalkleanleft.hkx"),
    ("walkforward.hkx", "sneakwalkleanright.hkx"),
    ("runforward.hkx", "sneakrunleanleft.hkx"),
    ("runforward.hkx", "sneakrunleanright.hkx"),
    // Knockdown / essential / crit poses.
    ("posea_idle1.hkx", "essentialdown.hkx"),
    ("posea_idle1.hkx", "essentialdownexit.hkx"),
    ("posea_idle1.hkx", "critbothlegsfalldown.hkx"),
];

const MELEE_ANIMATION_ALIASES: &[(&str, &str)] = &[
    ("attackforwarda.hkx", "attackbackwarda.hkx"),
    ("attackforwardb.hkx", "attackbackwardb.hkx"),
    (
        "attackforwardpower.hkx",
        "attackbackwardpowerfromrunning.hkx",
    ),
    ("attackforwardpower.hkx", "attackleftpowerfromrunning.hkx"),
    ("attackforwardpower.hkx", "attackrightpowerfromrunning.hkx"),
    ("attackforwardpower.hkx", "attacksprinting.hkx"),
    ("posea_idleflavor1.hkx", "idlealt_01.hkx"),
    ("posea_idleflavor3.hkx", "idlealt_03.hkx"),
    ("runbackward.hkx", "runbackwardleft.hkx"),
    ("runbackward.hkx", "runbackwardright.hkx"),
    ("runleft.hkx", "runbackwardleftforwardfacing.hkx"),
    ("runright.hkx", "runbackwardrightforwardfacing.hkx"),
    ("runforward.hkx", "runforwardleft.hkx"),
    ("runforward.hkx", "runforwardright.hkx"),
    // locomotion_8wayblend walk diagonals — combat approach paths diagonally, so a melee
    // stance without them tracks the target but cannot close (MoleMiner census).
    ("walkforward.hkx", "walkforwardleft.hkx"),
    ("walkforward.hkx", "walkforwardright.hkx"),
    ("walkbackward.hkx", "walkbackwardleft.hkx"),
    ("walkbackward.hkx", "walkbackwardright.hkx"),
    // Melee relaxed-stance locomotion.
    ("walkforward.hkx", "walkforwardrelaxed.hkx"),
    ("walkforward.hkx", "walkforwardleftrelaxed.hkx"),
    ("walkforward.hkx", "walkforwardrightrelaxed.hkx"),
    ("walkbackward.hkx", "walkbackwardrelaxed.hkx"),
    ("walkbackward.hkx", "walkbackwardleftrelaxed.hkx"),
    ("walkbackward.hkx", "walkbackwardrightrelaxed.hkx"),
    ("walkleft.hkx", "walkleftrelaxed.hkx"),
    ("walkright.hkx", "walkrightrelaxed.hkx"),
    // Jumps and ledge falls.
    ("runforward.hkx", "jumprun.hkx"),
    ("posea_idle1.hkx", "jumprunfall.hkx"),
    ("posea_idle1.hkx", "jumprunland.hkx"),
    ("posea_idle1.hkx", "jumprunimpactland.hkx"),
    ("posea_idle1.hkx", "jumpimpactland.hkx"),
    ("posea_idle1.hkx", "jumpinplace.hkx"),
    ("posea_idle1.hkx", "jumpinplacefall.hkx"),
    ("posea_idle1.hkx", "jumpinplaceland.hkx"),
    ("staggerbackmedium.hkx", "staggerblended.hkx"),
    // Combat cover states.
    ("posea_idle1.hkx", "covercrouchingenter.hkx"),
    ("posea_idle1.hkx", "covercrouchingexit.hkx"),
    ("posea_idle1.hkx", "covercrouchingidle.hkx"),
    ("posea_idle1.hkx", "covercrouchingpeekright.hkx"),
    ("posea_idle1.hkx", "covercrouchingpeekup.hkx"),
    ("posea_idle1.hkx", "coverstandingenter.hkx"),
    ("posea_idle1.hkx", "coverstandingexit.hkx"),
    ("posea_idle1.hkx", "coverstandingidle.hkx"),
    ("posea_idle1.hkx", "coverstandingpeekright.hkx"),
    // Sneak stance.
    ("posea_idle1.hkx", "sneakidle.hkx"),
    ("posea_idle1.hkx", "idletosneaktrans.hkx"),
    ("posea_idle1.hkx", "sneaktoidletrans.hkx"),
    ("walkforward.hkx", "sneakwalkforward.hkx"),
    ("walkforward.hkx", "sneakwalkforwardleft.hkx"),
    ("walkforward.hkx", "sneakwalkforwardright.hkx"),
    ("walkbackward.hkx", "sneakwalkbackward.hkx"),
    ("walkbackward.hkx", "sneakwalkbackwardleft.hkx"),
    ("walkbackward.hkx", "sneakwalkbackwardright.hkx"),
    ("walkleft.hkx", "sneakwalkleft.hkx"),
    ("walkright.hkx", "sneakwalkright.hkx"),
    ("runforward.hkx", "sneakrunforward.hkx"),
    ("runforward.hkx", "sneakrunforwardleft.hkx"),
    ("runforward.hkx", "sneakrunforwardright.hkx"),
    ("runbackward.hkx", "sneakrunbackward.hkx"),
    ("runbackward.hkx", "sneakrunbackwardleft.hkx"),
    ("runbackward.hkx", "sneakrunbackwardright.hkx"),
    ("runleft.hkx", "sneakrunleft.hkx"),
    ("runright.hkx", "sneakrunright.hkx"),
    ("turninplaceleft90.hkx", "sneakturninplaceleft90.hkx"),
    ("turninplaceright90.hkx", "sneakturninplaceright90.hkx"),
    ("turninplaceleft180.hkx", "sneakturninplaceleft180.hkx"),
    ("turninplaceright180.hkx", "sneakturninplaceright180.hkx"),
    // Relaxed turns and stance transitions.
    ("turninplaceleft90.hkx", "relaxedturninplaceleft90.hkx"),
    ("turninplaceright90.hkx", "relaxedturninplaceright90.hkx"),
    ("posea_idle1.hkx", "readytorelaxed.hkx"),
    ("posea_idle1.hkx", "sneakwpnidleready.hkx"),
    // Attack variants the shared melee graph exposes.
    ("attackforwarda.hkx", "attack180behind.hkx"),
    ("attackstandinga.hkx", "attackripper.hkx"),
    ("attackstandinga.hkx", "attackrippersneak.hkx"),
    ("attackstandinga.hkx", "attackparalyzingpalm.hkx"),
    ("dodgeleft.hkx", "dodgeback.hkx"),
    // Knockdown / essential / crit poses.
    ("posea_idle1.hkx", "essentialdown.hkx"),
    ("posea_idle1.hkx", "essentialdownexit.hkx"),
    ("posea_idle1.hkx", "critbothlegsfalldown.hkx"),
    ("wpngrenadethrow.hkx", "wpnminethrow.hkx"),
];

pub struct SynthesizeWeaponAnimationAliasesFixup;

impl Fixup for SynthesizeWeaponAnimationAliasesFixup {
    fn name(&self) -> &'static str {
        "synthesize_weapon_animation_aliases"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::AssetOnly
    }

    fn asset_phase_allowed(&self, phases: &AssetPhaseFlags) -> bool {
        phases.animations
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        config.mod_path.is_some() && config.target_extracted_dir.is_some()
    }

    fn run_with_session(
        &self,
        _session: &mut PluginSession,
        _mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let Some(mod_path) = config.mod_path.as_deref() else {
            return Ok(FixupReport::empty());
        };
        let Some(target_extracted_dir) = config.target_extracted_dir.as_deref() else {
            return Ok(FixupReport::empty());
        };
        synthesize_weapon_animation_aliases_in_mod_path(mod_path, target_extracted_dir)
    }
}

pub fn synthesize_weapon_animation_aliases_in_mod_path(
    mod_path: &Path,
    target_extracted_dir: &Path,
) -> Result<FixupReport, FixupError> {
    // Swept BEFORE the alias walk so a weapon whose FO76 source really does carry the
    // annotation still gets its correct master written in the same pass.
    let mut removed = 0u32;
    if let Some(target_meshes_root) = meshes_root_for_extracted_dir(target_extracted_dir) {
        for mod_meshes_root in mesh_roots_for_mod_path(mod_path) {
            remove_chain_reachable_fire_master_clones(
                &mod_meshes_root,
                &target_meshes_root,
                &mut removed,
            )?;
        }
    }

    let mut report = synthesize_animation_aliases_in_mod_path(
        mod_path,
        target_extracted_dir,
        WEAPON_ANIMATION_ALIASES,
        None,
    )?;
    report.records_changed += removed;
    Ok(report)
}

pub fn synthesize_mt_animation_aliases_in_mod_path(
    mod_path: &Path,
    target_extracted_dir: &Path,
) -> Result<FixupReport, FixupError> {
    synthesize_animation_aliases_in_mod_path(
        mod_path,
        target_extracted_dir,
        MT_ANIMATION_ALIASES,
        Some("MT"),
    )
}

pub fn synthesize_melee_animation_aliases_in_mod_path(
    mod_path: &Path,
    target_extracted_dir: &Path,
) -> Result<FixupReport, FixupError> {
    synthesize_animation_aliases_in_mod_path(
        mod_path,
        target_extracted_dir,
        MELEE_ANIMATION_ALIASES,
        Some("H2H"),
    )
}

fn synthesize_animation_aliases_in_mod_path(
    mod_path: &Path,
    target_extracted_dir: &Path,
    aliases: &[(&str, &str)],
    animation_dir_name: Option<&str>,
) -> Result<FixupReport, FixupError> {
    let Some(target_meshes_root) = meshes_root_for_extracted_dir(target_extracted_dir) else {
        return Ok(FixupReport::empty());
    };

    let mut changed = 0u32;
    let mut donors = DonorCache::new();
    for mod_meshes_root in mesh_roots_for_mod_path(mod_path) {
        synthesize_aliases_in_tree(
            &mod_meshes_root,
            &mod_meshes_root,
            &target_meshes_root,
            aliases,
            animation_dir_name,
            &mut changed,
            &mut donors,
        )?;
    }

    Ok(FixupReport {
        records_changed: changed,
        ..FixupReport::empty()
    })
}

fn synthesize_aliases_in_tree(
    dir: &Path,
    mod_meshes_root: &Path,
    target_meshes_root: &Path,
    aliases: &[(&str, &str)],
    animation_dir_name: Option<&str>,
    changed: &mut u32,
    donors: &mut DonorCache,
) -> Result<(), FixupError> {
    if dir
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("_1stperson"))
    {
        return Ok(());
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            synthesize_aliases_in_tree(
                &path,
                mod_meshes_root,
                target_meshes_root,
                aliases,
                animation_dir_name,
                changed,
                donors,
            )?;
        }
    }

    let Ok(relative_dir) = dir.strip_prefix(mod_meshes_root) else {
        return Ok(());
    };
    if !is_actor_animation_dir(relative_dir)
        || animation_dir_name.is_some_and(|name| !is_animation_dir(relative_dir, name))
    {
        return Ok(());
    }

    for (source_name, target_name) in aliases {
        let Some(source) = file_named(dir, source_name) else {
            continue;
        };
        let vanilla_owns = target_asset_exists(target_meshes_root, relative_dir, target_name);
        let write_dir = dir;

        // A fire clip already on disk that raises no event is either a stale synthesized
        // clone from an earlier run or an inert FO76 master. Both need repairing, and the
        // plain "already exists" skip would leave them broken across every future regen —
        // the mod's mesh tree is not wiped between runs, so a create-only fix is inert.
        let existing = file_named(write_dir, target_name);
        let repairable = existing.as_ref().is_some_and(|path| {
            requires_fire_annotation(target_name) && !contains_fire_annotation(path)
        });
        if !repairable && (existing.is_some() || vanilla_owns) {
            continue;
        }

        // FO4's `*ReadySlave` clips are the weapon-side half of the pair; only the
        // character-side master carries the `weaponFire` annotation that
        // `WeaponFireHandler::executeHandler` turns into `TESObjectWEAP::Fire`. A clone of
        // an unannotated slave animates and fires nothing, and shadows any correct clip
        // further down the SAPT chain (80 of 256 synthesized masters, including the M2 the
        // .50 Cal binds). FO76's Gauss slave does carry it, so the check is per-file.
        let source = if requires_fire_annotation(target_name) && !contains_fire_annotation(&source)
        {
            // A weapon set under `Character\Animations\Weapon\` falls through to its grip
            // set, then the generic actor root, so a stand-in in the weapon's own folder
            // shadows the better clip the chain would find. The shallowest donor is the
            // generic `Animations\wpnfireautoready.hkx`, which made the .50 Cal fire from a
            // rifle pose while `Weapon\GripHeavy` held the right one. Leave it absent.
            if is_character_weapon_set(relative_dir) {
                // The mod's mesh tree is not wiped between runs, so a stale inert master
                // written by an earlier regen would go on shadowing the chain forever.
                // Removing it is what makes this take effect on an existing tree.
                if repairable && let Some(stale) = existing.as_ref() {
                    std::fs::remove_file(stale).map_err(|error| {
                        FixupError::Other(format!(
                            "failed to remove inert weapon animation master {}: {error}",
                            stale.display()
                        ))
                    })?;
                    *changed += 1;
                }
                continue;
            }
            // Elsewhere, substitute rather than skip. Vanilla keeps these clips per weapon
            // folder — `supermutant/animations/shared` has none — so dropping the alias
            // would leave the graph with nothing to bind, which T-poses the actor while
            // firing (the very failure this whole table exists to prevent).
            match donors.fire_donor(target_meshes_root, relative_dir, target_name) {
                Some(donor) => donor,
                // No donor available. Creating the inert clip is still better than leaving
                // the graph nothing to bind, but REWRITING an existing one gains nothing —
                // leave it untouched so the pass stays idempotent.
                None if repairable => continue,
                None => source,
            }
        } else {
            source
        };

        let target = write_dir.join(target_name);
        std::fs::copy(&source, &target).map_err(|error| {
            FixupError::Other(format!(
                "failed to create weapon animation alias {} from {}: {error}",
                target.display(),
                source.display()
            ))
        })?;
        *changed += 1;
    }

    Ok(())
}

fn is_actor_animation_dir(relative_dir: &Path) -> bool {
    let components: Vec<_> = relative_dir
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();
    components
        .first()
        .is_some_and(|name| name.eq_ignore_ascii_case("Actors"))
        && components
            .iter()
            .any(|name| name.eq_ignore_ascii_case("Animations"))
}

fn is_animation_dir(relative_dir: &Path, expected: &str) -> bool {
    relative_dir
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(expected))
}

/// The annotation `WeaponFireHandler::executeHandler` keys on. It is stored as a literal
/// string in the clip's annotation track, so a byte search is sufficient and avoids
/// unpacking every candidate .hkx.
const WEAPON_FIRE_ANNOTATION: &[u8] = b"weaponFire";

/// The two third-person masters the engine binds a shot to.
const FIRE_MASTER_NAMES: &[&str] = &["wpnfireautoready.hkx", "wpnfiresingleready.hkx"];

/// Aliases whose whole purpose is to supply a FIRING clip, and which are therefore
/// worthless without the annotation. Deliberately keyed on the alias TARGET, not the
/// source: the same slave is a perfectly good donor for non-firing aliases.
fn requires_fire_annotation(target_name: &str) -> bool {
    FIRE_MASTER_NAMES
        .iter()
        .any(|name| target_name.eq_ignore_ascii_case(name))
}

/// Delete fire masters that are byte-identical copies of a vanilla clip the weapon's own
/// `SAPT` chain already reaches.
///
/// The mod's mesh tree persists between regens, so clones from earlier runs must be
/// removed, not skipped as "already exists". Removing one loses nothing, since the chain
/// still resolves the original, and it un-shadows the grip-specific clip in between (7
/// shipped weapons, the .50 Cal among them, carried a copy of
/// `Actors\Character\Animations\wpnfireautoready.hkx` and fired from the generic rifle
/// pose).
///
/// Keyed on content, never name or timestamp: a converted FO76 clip is never
/// byte-identical to a vanilla one (the base-game dedup drops those), so only clones match.
fn remove_chain_reachable_fire_master_clones(
    mod_meshes_root: &Path,
    target_meshes_root: &Path,
    changed: &mut u32,
) -> Result<(), FixupError> {
    let Some(weapon_root) = resolve_relative_dir(
        mod_meshes_root,
        Path::new("Actors/Character/Animations/Weapon"),
    ) else {
        return Ok(());
    };
    let Some(vanilla_root) =
        resolve_relative_dir(target_meshes_root, Path::new("Actors/Character"))
    else {
        return Ok(());
    };

    for name in FIRE_MASTER_NAMES {
        let mut vanilla = Vec::new();
        collect_clip_contents(&vanilla_root, name, &mut vanilla);
        if !vanilla.is_empty() {
            remove_clips_matching(&weapon_root, name, &vanilla, changed)?;
        }
    }
    Ok(())
}

fn collect_clip_contents(dir: &Path, file_name: &str, out: &mut Vec<Vec<u8>>) {
    if let Some(path) = file_named(dir, file_name)
        && let Ok(bytes) = std::fs::read(&path)
        && !out.contains(&bytes)
    {
        out.push(bytes);
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_clip_contents(&path, file_name, out);
        }
    }
}

fn remove_clips_matching(
    dir: &Path,
    file_name: &str,
    contents: &[Vec<u8>],
    changed: &mut u32,
) -> Result<(), FixupError> {
    if let Some(path) = file_named(dir, file_name)
        && std::fs::read(&path).is_ok_and(|bytes| contents.contains(&bytes))
    {
        std::fs::remove_file(&path).map_err(|error| {
            FixupError::Other(format!(
                "failed to remove chain-reachable weapon animation clone {}: {error}",
                path.display()
            ))
        })?;
        *changed += 1;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            remove_clips_matching(&path, file_name, contents, changed)?;
        }
    }
    Ok(())
}

fn contains_fire_annotation(path: &Path) -> bool {
    match std::fs::read(path) {
        Ok(bytes) => bytes
            .windows(WEAPON_FIRE_ANNOTATION.len())
            .any(|window| window == WEAPON_FIRE_ANNOTATION),
        Err(_) => false,
    }
}

/// Memoised lookup of a vanilla FO4 clip that can stand in for an annotation-less slave.
///
/// Scoped to the broken clip's actor (`Actors/<Actor>`), since a clip binds to one
/// skeleton (a human fire clip on a Super Mutant binds to nothing). Within it the
/// shallowest path wins, then lexicographic order: the shallowest annotated clip is the
/// actor's generic one (`Actors\Character\Animations\wpnfireautoready.hkx`), which
/// approximates where the SAPT chain falls through once weapon-specific folders miss.
struct DonorCache {
    entries: std::collections::HashMap<(PathBuf, String), Option<PathBuf>>,
}

impl DonorCache {
    fn new() -> Self {
        Self {
            entries: std::collections::HashMap::new(),
        }
    }

    fn fire_donor(
        &mut self,
        target_meshes_root: &Path,
        relative_dir: &Path,
        file_name: &str,
    ) -> Option<PathBuf> {
        let actor_root = actor_root_of(relative_dir)?;
        let key = (actor_root.clone(), file_name.to_ascii_lowercase());
        if let Some(cached) = self.entries.get(&key) {
            return cached.clone();
        }
        let donor = resolve_relative_dir(target_meshes_root, &actor_root)
            .and_then(|dir| best_annotated_clip(&dir, file_name));
        self.entries.insert(key, donor.clone());
        donor
    }
}

/// `Actors/Character/Animations/Weapon/M2/Player` -> `Actors/Character`.
fn actor_root_of(relative_dir: &Path) -> Option<PathBuf> {
    let mut components = relative_dir
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        });
    let actors = components.next()?;
    if !actors.eq_ignore_ascii_case("Actors") {
        return None;
    }
    let actor = components.next()?;
    Some(Path::new(actors).join(actor))
}

fn best_annotated_clip(dir: &Path, file_name: &str) -> Option<PathBuf> {
    let mut best: Option<(usize, PathBuf)> = None;
    collect_annotated_clips(dir, file_name, 0, &mut best);
    best.map(|(_, path)| path)
}

fn collect_annotated_clips(
    dir: &Path,
    file_name: &str,
    depth: usize,
    best: &mut Option<(usize, PathBuf)>,
) {
    if let Some(candidate) = file_named(dir, file_name)
        && contains_fire_annotation(&candidate)
        && best.as_ref().is_none_or(|(best_depth, best_path)| {
            depth < *best_depth || (depth == *best_depth && candidate < *best_path)
        })
    {
        *best = Some((depth, candidate));
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_annotated_clips(&path, file_name, depth + 1, best);
        }
    }
}

/// Whether `relative_dir` is a weapon set in the shared human tree
/// (`Actors\Character\Animations\Weapon\...`), which always has a grip set and the generic
/// actor root to fall through to. Creature trees do not, which is why they still need a
/// synthesized stand-in.
fn is_character_weapon_set(relative_dir: &Path) -> bool {
    let names: Vec<String> = relative_dir
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => name.to_str().map(|n| n.to_ascii_lowercase()),
            _ => None,
        })
        .collect();
    let mut windows = names.windows(2);
    names.first().is_some_and(|n| n == "actors")
        && names.get(1).is_some_and(|n| n == "character")
        && windows.any(|pair| pair[0] == "animations" && pair[1] == "weapon")
}

fn target_asset_exists(target_meshes_root: &Path, relative_dir: &Path, file_name: &str) -> bool {
    let Some(target_dir) = resolve_relative_dir(target_meshes_root, relative_dir) else {
        return false;
    };
    file_named(&target_dir, file_name).is_some()
}

fn resolve_relative_dir(root: &Path, relative_dir: &Path) -> Option<PathBuf> {
    let mut current = root.to_path_buf();
    for component in relative_dir.components() {
        let Component::Normal(expected) = component else {
            return None;
        };
        let expected = expected.to_str()?;
        current = directory_named(&current, expected)?;
    }
    Some(current)
}

fn mesh_roots_for_mod_path(mod_path: &Path) -> Vec<PathBuf> {
    [
        mod_path.join("data").join("Meshes"),
        mod_path.join("meshes"),
    ]
    .into_iter()
    .filter(|path| path.is_dir())
    .collect()
}

fn meshes_root_for_extracted_dir(target_extracted_dir: &Path) -> Option<PathBuf> {
    if target_extracted_dir
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("Meshes"))
    {
        return Some(target_extracted_dir.to_path_buf());
    }
    directory_named(target_extracted_dir, "Meshes")
}

fn directory_named(dir: &Path, expected: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().find_map(|entry| {
        let path = entry.path();
        (path.is_dir()
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.eq_ignore_ascii_case(expected)))
        .then_some(path)
    })
}

fn file_named(dir: &Path, expected: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().find_map(|entry| {
        let path = entry.path();
        (path.is_file()
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.eq_ignore_ascii_case(expected)))
        .then_some(path)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn creates_fo4_fire_aliases_for_custom_actor_animations() {
        let temp = tempfile::tempdir().unwrap();
        let mod_path = temp.path().join("mod");
        let target = temp.path().join("target");
        let animations = mod_path.join("data/Meshes/Actors/MoleMiner/Animations/GripAssault");
        std::fs::create_dir_all(target.join("Meshes")).unwrap();

        // Pairs whose target doubles as another pair's source (the base sneak aliases the
        // derived entries chain from) are pre-written below and therefore skipped.
        let sources: std::collections::HashSet<&str> = WEAPON_ANIMATION_ALIASES
            .iter()
            .map(|(source, _)| *source)
            .collect();
        for (source_name, target_name) in WEAPON_ANIMATION_ALIASES {
            write_file(&animations.join(source_name), source_name.as_bytes());
            if !sources.contains(target_name) {
                assert!(!animations.join(target_name).exists());
            }
        }

        let expected: Vec<&(&str, &str)> = WEAPON_ANIMATION_ALIASES
            .iter()
            .filter(|(_, target)| !sources.contains(target))
            .collect();
        let report = synthesize_weapon_animation_aliases_in_mod_path(&mod_path, &target).unwrap();
        assert_eq!(report.records_changed, expected.len() as u32);
        for (source_name, target_name) in expected {
            assert_eq!(
                std::fs::read(animations.join(target_name)).unwrap(),
                source_name.as_bytes()
            );
        }

        let second = synthesize_weapon_animation_aliases_in_mod_path(&mod_path, &target).unwrap();
        assert_eq!(second.records_changed, 0);
    }

    #[test]
    fn creates_fo4_mt_lean_aliases_for_custom_actor_locomotion() {
        let temp = tempfile::tempdir().unwrap();
        let mod_path = temp.path().join("mod");
        let target = temp.path().join("target");
        let animations = mod_path.join("data/Meshes/Actors/MoleMiner/Animations/MT");
        std::fs::create_dir_all(target.join("Meshes")).unwrap();

        for (source_name, target_name) in MT_ANIMATION_ALIASES {
            write_file(&animations.join(source_name), source_name.as_bytes());
            assert!(!animations.join(target_name).exists());
        }

        let report = synthesize_mt_animation_aliases_in_mod_path(&mod_path, &target).unwrap();
        assert_eq!(report.records_changed, MT_ANIMATION_ALIASES.len() as u32);
        for (source_name, target_name) in MT_ANIMATION_ALIASES {
            assert_eq!(
                std::fs::read(animations.join(target_name)).unwrap(),
                source_name.as_bytes()
            );
        }

        let second = synthesize_mt_animation_aliases_in_mod_path(&mod_path, &target).unwrap();
        assert_eq!(second.records_changed, 0);
    }

    #[test]
    fn creates_fo4_melee_aliases_only_in_h2h_folders() {
        let temp = tempfile::tempdir().unwrap();
        let mod_path = temp.path().join("mod");
        let target = temp.path().join("target");
        let h2h = mod_path.join("data/Meshes/Actors/MoleMiner/Animations/H2H");
        let mt = mod_path.join("data/Meshes/Actors/MoleMiner/Animations/MT");
        std::fs::create_dir_all(target.join("Meshes")).unwrap();

        for (source_name, target_name) in MELEE_ANIMATION_ALIASES {
            write_file(&h2h.join(source_name), source_name.as_bytes());
            write_file(&mt.join(source_name), b"wrong-folder");
            assert!(!h2h.join(target_name).exists());
        }

        let report = synthesize_melee_animation_aliases_in_mod_path(&mod_path, &target).unwrap();
        assert_eq!(report.records_changed, MELEE_ANIMATION_ALIASES.len() as u32);
        for (source_name, target_name) in MELEE_ANIMATION_ALIASES {
            assert_eq!(
                std::fs::read(h2h.join(target_name)).unwrap(),
                source_name.as_bytes()
            );
            assert!(!mt.join(target_name).exists());
        }

        let second = synthesize_melee_animation_aliases_in_mod_path(&mod_path, &target).unwrap();
        assert_eq!(second.records_changed, 0);
    }

    /// Production MoleMiner shape: the melee 8-way walk blend needs the four diagonals the
    /// FO76 H2H folder never authored — combat approach paths diagonally, so without them
    /// melee-stance actors track but cannot close.
    #[test]
    fn melee_walk_diagonals_alias_from_cardinals() {
        let temp = tempfile::tempdir().unwrap();
        let mod_path = temp.path().join("mod");
        let target = temp.path().join("target");
        let h2h = mod_path.join("data/Meshes/Actors/MoleMiner/Animations/H2H");
        std::fs::create_dir_all(target.join("Meshes")).unwrap();
        write_file(&h2h.join("walkforward.hkx"), b"fwd");
        write_file(&h2h.join("walkbackward.hkx"), b"back");

        synthesize_melee_animation_aliases_in_mod_path(&mod_path, &target).unwrap();

        for name in [
            "walkforwardleft.hkx",
            "walkforwardright.hkx",
            "walkbackwardleft.hkx",
            "walkbackwardright.hkx",
        ] {
            assert!(
                h2h.join(name).is_file(),
                "missing walk diagonal alias {name}"
            );
        }
    }

    /// Production MoleMiner shape: gun folders ship only standing wpn* clips. The base sneak
    /// aliases must materialize from standing donors, and the pre-existing sneak-derived
    /// entries must chain off them within the same pass (table order).
    #[test]
    fn sneak_gun_aliases_chain_from_standing_clips() {
        let temp = tempfile::tempdir().unwrap();
        let mod_path = temp.path().join("mod");
        let target = temp.path().join("target");
        let gun = mod_path.join("data/Meshes/Actors/MoleMiner/Animations/GripAssault");
        std::fs::create_dir_all(target.join("Meshes")).unwrap();
        write_file(&gun.join("wpnidleready.hkx"), b"idle");
        write_file(&gun.join("wpnfireautoready.hkx"), b"auto");

        synthesize_weapon_animation_aliases_in_mod_path(&mod_path, &target).unwrap();

        assert_eq!(
            std::fs::read(gun.join("sneakwpnidleready.hkx")).unwrap(),
            b"idle"
        );
        assert_eq!(
            std::fs::read(gun.join("sneakwpnidleready_left.hkx")).unwrap(),
            b"idle",
            "derived sneak alias must chain off the base sneak alias created in the same pass"
        );
        assert_eq!(
            std::fs::read(gun.join("sneakwpnfireautoready_left.hkx")).unwrap(),
            b"auto"
        );
    }

    #[test]
    fn preserves_existing_mod_and_fo4_target_clips() {
        let temp = tempfile::tempdir().unwrap();
        let mod_path = temp.path().join("mod");
        let target = temp.path().join("target");
        let relative = Path::new("Actors/MoleMiner/Animations/GripAssault");
        let animations = mod_path.join("data/Meshes").join(relative);
        let target_animations = target.join("Meshes").join(relative);

        write_file(&animations.join("wpnfireauto_additive.hkx"), b"source-auto");
        write_file(
            &animations.join("wpnfireautoready.hkx"),
            b"mod-auto weaponFire",
        );
        write_file(
            &animations.join("wpnfiresingle_additive.hkx"),
            b"source-single",
        );
        write_file(
            &target_animations.join("WPNFireSingleReady.hkx"),
            b"fo4-single",
        );

        // Not asserted on `records_changed`: the pre-written master is itself the SOURCE of
        // the sneak aliases, so unrelated entries legitimately fire here.
        synthesize_weapon_animation_aliases_in_mod_path(&mod_path, &target).unwrap();
        assert_eq!(
            std::fs::read(animations.join("wpnfireautoready.hkx")).unwrap(),
            b"mod-auto weaponFire"
        );
        assert!(!animations.join("wpnfiresingleready.hkx").exists());
    }

    #[test]
    fn skips_first_person_weapon_animations() {
        let temp = tempfile::tempdir().unwrap();
        let mod_path = temp.path().join("mod");
        let target = temp.path().join("target");
        let animations =
            mod_path.join("data/Meshes/Actors/Character/_1stPerson/Animations/TestWeapon");
        std::fs::create_dir_all(target.join("Meshes")).unwrap();
        write_file(
            &animations.join("wpnfiresinglereadyslave.hkx"),
            b"first-person",
        );

        let report = synthesize_weapon_animation_aliases_in_mod_path(&mod_path, &target).unwrap();
        assert_eq!(report.records_changed, 0);
        assert!(!animations.join("wpnfiresingleready.hkx").exists());
    }

    // The `*ReadySlave` clip is the weapon-side half of the pair. Only the character-side
    // master carries `weaponFire`, which is what the engine turns into TESObjectWEAP::Fire,
    // so cloning an inert slave produces a weapon that is completely silent in third person.
    #[test]
    fn inert_fire_slave_is_replaced_by_an_annotated_vanilla_donor() {
        let temp = tempfile::tempdir().unwrap();
        let mod_path = temp.path().join("mod");
        let target = temp.path().join("target");
        let animations = mod_path.join("data/Meshes/Actors/Supermutant/Animations/Incinerator");
        // Vanilla keeps the annotated master in a sibling folder, never in the weapon's own.
        let vanilla = target.join("Meshes/Actors/Supermutant/Animations/Shared");
        write_file(
            &animations.join("wpnfireautoreadyslave.hkx"),
            b"inert slave",
        );
        write_file(
            &vanilla.join("wpnfireautoready.hkx"),
            b"donor weaponFire clip",
        );

        synthesize_weapon_animation_aliases_in_mod_path(&mod_path, &target).unwrap();

        assert_eq!(
            std::fs::read(animations.join("wpnfireautoready.hkx")).unwrap(),
            b"donor weaponFire clip",
        );
    }

    // In a Character weapon set a stand-in in the weapon's own folder shadows the better
    // clip further down the chain (the generic donor made the .50 Cal fire from a rifle
    // pose while `Weapon\GripHeavy` held the right one), so none is planted.
    #[test]
    fn inert_fire_slave_in_a_character_weapon_set_is_left_to_the_chain() {
        let temp = tempfile::tempdir().unwrap();
        let mod_path = temp.path().join("mod");
        let target = temp.path().join("target");
        let animations = mod_path.join("data/Meshes/Actors/Character/Animations/Weapon/M2/Player");
        let generic = target.join("Meshes/Actors/Character/Animations");
        write_file(
            &animations.join("wpnfireautoreadyslave.hkx"),
            b"inert slave",
        );
        write_file(
            &generic.join("wpnfireautoready.hkx"),
            b"generic weaponFire clip",
        );

        synthesize_weapon_animation_aliases_in_mod_path(&mod_path, &target).unwrap();

        assert!(
            !animations.join("wpnfireautoready.hkx").exists(),
            "a generic donor must not be planted in a Character weapon set"
        );
    }

    // FO76's Gauss Pistol slave carries the annotation, so the check is per-file and an
    // annotated slave is still cloned.
    #[test]
    fn annotated_fire_slave_is_still_cloned_verbatim() {
        let temp = tempfile::tempdir().unwrap();
        let mod_path = temp.path().join("mod");
        let target = temp.path().join("target");
        let animations = mod_path.join("data/Meshes/Actors/Character/Animations/Weapon/Gauss");
        let vanilla = target.join("Meshes/Actors/Character/Animations/Weapon/GripHeavy");
        write_file(
            &animations.join("wpnfireautoreadyslave.hkx"),
            b"slave with weaponFire inside",
        );
        write_file(
            &vanilla.join("wpnfireautoready.hkx"),
            b"donor weaponFire clip",
        );

        synthesize_weapon_animation_aliases_in_mod_path(&mod_path, &target).unwrap();

        assert_eq!(
            std::fs::read(animations.join("wpnfireautoready.hkx")).unwrap(),
            b"slave with weaponFire inside",
        );
    }

    // Vanilla stores these per weapon folder — `supermutant/animations/shared` has none — so
    // when no donor exists the inert slave must still be copied. Leaving the graph with no
    // binding at all T-poses the actor while firing, which is worse than a silent shot.
    #[test]
    fn inert_fire_slave_is_kept_when_no_donor_exists() {
        let temp = tempfile::tempdir().unwrap();
        let mod_path = temp.path().join("mod");
        let target = temp.path().join("target");
        let animations = mod_path.join("data/Meshes/Actors/Supermutant/Animations/Incinerator");
        std::fs::create_dir_all(target.join("Meshes")).unwrap();
        write_file(
            &animations.join("wpnfireautoreadyslave.hkx"),
            b"inert slave",
        );

        synthesize_weapon_animation_aliases_in_mod_path(&mod_path, &target).unwrap();

        assert_eq!(
            std::fs::read(animations.join("wpnfireautoready.hkx")).unwrap(),
            b"inert slave",
        );
    }

    // The mod's mesh tree survives between regens, so an inert master from an earlier run
    // must be repaired rather than skipped as "already exists".
    #[test]
    fn stale_inert_master_from_an_earlier_run_is_repaired() {
        let temp = tempfile::tempdir().unwrap();
        let mod_path = temp.path().join("mod");
        let target = temp.path().join("target");
        let animations = mod_path.join("data/Meshes/Actors/Supermutant/Animations/Incinerator");
        let vanilla = target.join("Meshes/Actors/Supermutant/Animations/Shared");
        write_file(
            &animations.join("wpnfireautoreadyslave.hkx"),
            b"inert slave",
        );
        // Left behind by a previous regen, before the annotation gate existed.
        write_file(&animations.join("wpnfireautoready.hkx"), b"inert slave");
        write_file(
            &vanilla.join("wpnfireautoready.hkx"),
            b"donor weaponFire clip",
        );

        synthesize_weapon_animation_aliases_in_mod_path(&mod_path, &target).unwrap();

        assert_eq!(
            std::fs::read(animations.join("wpnfireautoready.hkx")).unwrap(),
            b"donor weaponFire clip",
        );
    }

    // The masters the OLD donor path planted are annotated — they are verbatim copies of
    // vanilla's generic clip — so the "no weaponFire" repair gate never fires on them and
    // the "already exists" skip left all 9 of them in the shipped tree. Content is the only
    // test that catches them.
    #[test]
    fn fire_masters_cloned_from_a_chain_reachable_vanilla_clip_are_removed() {
        let temp = tempfile::tempdir().unwrap();
        let mod_path = temp.path().join("mod");
        let target = temp.path().join("target");
        let m2 = mod_path.join("data/Meshes/Actors/Character/Animations/Weapon/M2/Player");
        let generic = target.join("Meshes/Actors/Character/Animations");

        write_file(&generic.join("wpnfireautoready.hkx"), b"generic weaponFire");
        // What an earlier regen's donor substitution left behind, byte for byte.
        write_file(&m2.join("wpnfireautoready.hkx"), b"generic weaponFire");
        // A genuinely converted FO76 master is never a byte match, so it must survive.
        write_file(
            &m2.join("wpnfiresingleready.hkx"),
            b"converted FO76 weaponFire",
        );
        write_file(
            &generic.join("wpnfiresingleready.hkx"),
            b"other vanilla clip",
        );

        synthesize_weapon_animation_aliases_in_mod_path(&mod_path, &target).unwrap();

        assert!(
            !m2.join("wpnfireautoready.hkx").exists(),
            "a verbatim clone of a chain-reachable vanilla clip must be removed"
        );
        assert_eq!(
            std::fs::read(m2.join("wpnfiresingleready.hkx")).unwrap(),
            b"converted FO76 weaponFire",
            "a converted master is not a clone and must be kept"
        );
    }

    // In the Character weapon tree the repair is a deletion, not a rewrite: the stale master
    // shadows the grip set's correct clip, and even a donor-repaired one reproduces the .50
    // Cal's wrong standing fire pose.
    #[test]
    fn stale_inert_master_in_a_character_weapon_set_is_removed() {
        let temp = tempfile::tempdir().unwrap();
        let mod_path = temp.path().join("mod");
        let target = temp.path().join("target");
        let animations = mod_path.join("data/Meshes/Actors/Character/Animations/Weapon/M2/Player");
        let vanilla = target.join("Meshes/Actors/Character/Animations/Weapon/GripHeavy");
        write_file(
            &animations.join("wpnfireautoreadyslave.hkx"),
            b"inert slave",
        );
        write_file(&animations.join("wpnfireautoready.hkx"), b"inert slave");
        write_file(
            &vanilla.join("wpnfireautoready.hkx"),
            b"donor weaponFire clip",
        );

        synthesize_weapon_animation_aliases_in_mod_path(&mod_path, &target).unwrap();

        assert!(
            !animations.join("wpnfireautoready.hkx").exists(),
            "stale inert master must be removed so the SAPT chain can reach GripHeavy"
        );
    }

    // Repair must not turn into churn: an already-annotated master is left alone, and an
    // inert one with no donor available is left alone too.
    #[test]
    fn existing_masters_are_left_alone_when_repair_would_not_help() {
        let temp = tempfile::tempdir().unwrap();
        let mod_path = temp.path().join("mod");
        let target = temp.path().join("target");
        let good = mod_path.join("data/Meshes/Actors/Character/Animations/Weapon/Gauss");
        let no_donor = mod_path.join("data/Meshes/Actors/Supermutant/Animations/Incinerator");
        std::fs::create_dir_all(target.join("Meshes")).unwrap();
        write_file(&good.join("wpnfireautoreadyslave.hkx"), b"slave");
        write_file(
            &good.join("wpnfireautoready.hkx"),
            b"existing weaponFire master",
        );
        write_file(&no_donor.join("wpnfireautoreadyslave.hkx"), b"inert slave");
        write_file(&no_donor.join("wpnfireautoready.hkx"), b"inert master");

        // Not asserted on `records_changed`: the pre-written master is itself the SOURCE of
        // the sneak/sighted aliases, so unrelated entries legitimately fire here.
        synthesize_weapon_animation_aliases_in_mod_path(&mod_path, &target).unwrap();

        assert_eq!(
            std::fs::read(good.join("wpnfireautoready.hkx")).unwrap(),
            b"existing weaponFire master",
        );
        assert_eq!(
            std::fs::read(no_donor.join("wpnfireautoready.hkx")).unwrap(),
            b"inert master",
        );
    }

    // A clip is authored against one skeleton, so a human donor handed to a Super Mutant would
    // bind to nothing. The donor search must not cross the actor boundary.
    #[test]
    fn donor_search_does_not_cross_actor_boundaries() {
        let temp = tempfile::tempdir().unwrap();
        let mod_path = temp.path().join("mod");
        let target = temp.path().join("target");
        let animations = mod_path.join("data/Meshes/Actors/Supermutant/Animations/Incinerator");
        let human = target.join("Meshes/Actors/Character/Animations/Weapon/GripHeavy");
        write_file(
            &animations.join("wpnfireautoreadyslave.hkx"),
            b"inert slave",
        );
        write_file(
            &human.join("wpnfireautoready.hkx"),
            b"human weaponFire clip",
        );

        synthesize_weapon_animation_aliases_in_mod_path(&mod_path, &target).unwrap();

        assert_eq!(
            std::fs::read(animations.join("wpnfireautoready.hkx")).unwrap(),
            b"inert slave",
        );
    }
}
