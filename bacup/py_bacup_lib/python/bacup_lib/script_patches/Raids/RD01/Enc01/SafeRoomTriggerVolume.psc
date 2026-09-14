Event OnTriggerEnter(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer() && IMod != None
		IsActive = True
		IMod.Apply()
	EndIf
EndEvent

Event OnTriggerLeave(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer() && IsActive
		IsActive = False
		If IMod != None
			IMod.Remove()
		EndIf
	EndIf
EndEvent
