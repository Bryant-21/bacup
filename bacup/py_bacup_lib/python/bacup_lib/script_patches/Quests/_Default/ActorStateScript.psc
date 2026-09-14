; FO76 Actor.SetBloodImpactMaterial(Material) swapped the impact/blood decal material
; an actor uses per state (e.g. metallic sparks while invulnerable). Fallout 4 exposes
; no per-actor impact-material override in Papyrus, so both client hooks become no-ops.
; BEHAVIOUR LOST: per-state blood/impact material swapping. The ActorState struct still
; carries BloodImpactMaterial, so this is recoverable if an F4SE hook is ever added.

Function _EndStateEffectsClient(Int oldIndex, Int newIndex)
EndFunction

Function _BeginStateEffectsClient(Int newIndex, Int oldIndex)
EndFunction
