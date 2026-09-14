Event OnTriggerEnter(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer() && DebugMarker != None
		akActionRef.MoveTo(DebugMarker)
	EndIf
EndEvent
