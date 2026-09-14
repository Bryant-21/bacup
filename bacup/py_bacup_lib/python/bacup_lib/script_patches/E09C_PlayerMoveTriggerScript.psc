Event OnTriggerEnter(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer()
		ObjectReference destination = GetLinkedRef()
		If destination != None
			akActionRef.MoveTo(destination)
		EndIf
	EndIf
EndEvent
