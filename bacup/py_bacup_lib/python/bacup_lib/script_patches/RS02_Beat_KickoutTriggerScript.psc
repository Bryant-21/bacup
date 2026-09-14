Event OnTriggerEnter(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer() && myLocation != None
		akActionRef.MoveTo(myLocation)
	EndIf
EndEvent
