; FO76 Actor.SetBloodImpactMaterial(Material) swapped the impact/blood decal material
; per state -- here, metallic "ping" impacts while the boss is invulnerable. Fallout 4
; exposes no per-actor impact-material override in Papyrus.
; BEHAVIOUR LOST: hits on the invulnerable Assaultron boss no longer read visually as
; deflected. InvulnerableImpactMaterial remains bound on the record.

State invulnerable
	Event OnBeginState(String asOldState)
	EndEvent
EndState

State vulnerable
	Event OnBeginState(String asOldState)
	EndEvent
EndState
