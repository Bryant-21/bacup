Event OnActivate(ObjectReference akActionRef)
	If akActionRef != Game.GetPlayer()
		Return
	EndIf
	Quest owningQuest = GetOwningQuest()
	E09B_Script wheelScript = owningQuest as E09B_Script
	If wheelScript != None
		wheelScript.HandleCappyFound(GetReference())
	EndIf
EndEvent
