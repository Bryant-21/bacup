Event OnActivate(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer()
		ObjectReference aliasRef = GetReference()
		If aliasRef != None
			aliasRef.Disable()
			Quest owningQuest = GetOwningQuest()
			E09B_Script wheelScript = owningQuest as E09B_Script
			If wheelScript != None
				wheelScript.HandleChickenCaught(aliasRef)
			EndIf
		EndIf
	EndIf
EndEvent
