; VisualEffect.UseOcclusionMap and SetAltitudeFollowsStatics (statics-following particle
; altitude / occlusion map) are FO76-only renderer features with no FO4 counterpart, and
; FO4's SpellApplyCameraAttachedFXScript has no useOcclusionMap property. The attached
; camera effect itself is still played on the player.

Event OnEffectStart(Actor akTarget, Actor akCaster)
    If akTarget == Game.GetPlayer()
        camAttachedFX.Play(akTarget, timeToApply, None)
    EndIf
EndEvent
