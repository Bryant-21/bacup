; FO76 Actor.FindRandomCombatTarget(radius) enumerated valid hostile targets around
; the QA bot. Fallout 4 has no equivalent (its Find*Reference* globals all require a
; base form). BEHAVIOUR LOST: automated target seeking; CheckCombat now always reports
; "no target".
Bool Function CheckCombat()
	Return False
EndFunction
