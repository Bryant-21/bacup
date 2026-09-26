Event OnActivate(ObjectReference akActionRef)
	If akActionRef != Game.GetPlayer()
		Return
	EndIf
	Quest owningQuest = GetOwningQuest()
	E09B_Script wheelScript = owningQuest as E09B_Script
	; The game show spins five times, so each prompt re-arms the press through E09B_Script.canSpin.
	If wheelScript == None || !wheelScript.TryBeginSpin()
		Return
	EndIf
	owningQuest.SetStage(170)
EndEvent

Event OnAliasShutdown()
	ObjectReference buttonRef = GetReference()
	If buttonRef != None
		buttonRef.PlayAnimation("TurnOff01")
	EndIf
	isFirstPress = True
EndEvent
