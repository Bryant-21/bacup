Event OnDeath(Actor akKiller)
	ApplyAVOnDeath()
EndEvent

Event OnDying(Actor akKiller)
	If UseOnDying
		ApplyAVOnDeath()
	EndIf
EndEvent

Function ApplyAVOnDeath()
	If AVToSet == None
		Return
	EndIf
	(Self as Actor).SetValue(AVToSet, AVNewValue)
	If DejaChannel != ""
		; FO4's Debug.Trace takes (message, severity) — FO76's third Deja-channel
		; argument does not exist, so it goes into the message instead.
		Debug.Trace("[" + DejaChannel + "] " + Self as String + " DefaultActorSetAVOnDeathInstOwner| Set " + AVToSet as String + " = " + AVNewValue as String, 0)
	EndIf
EndFunction
