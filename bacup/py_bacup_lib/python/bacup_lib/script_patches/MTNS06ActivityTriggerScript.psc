Event OnTriggerEnter(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer() && PlayersInActivity != None
		PlayersInActivity.AddRef(akActionRef)
	EndIf
EndEvent

Event OnTriggerLeave(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer() && PlayersInActivity != None
		PlayersInActivity.RemoveRef(akActionRef)
	EndIf
EndEvent
