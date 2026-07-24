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
		Debug.Trace(Self as String + " DefaultActorSetAVOnDeathInstOwner| Set " + AVToSet as String + " = " + AVNewValue as String, 0, DejaChannel)
	EndIf
EndFunction
