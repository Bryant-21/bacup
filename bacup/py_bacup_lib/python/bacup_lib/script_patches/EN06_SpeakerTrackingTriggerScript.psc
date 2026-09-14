Event OnTriggerEnter(ObjectReference akActionRef)
	Actor speaker = akActionRef as Actor
	If speaker != None && CurrentSpeaker != None
		CurrentSpeaker.ForceRefTo(speaker)
	EndIf
EndEvent

Event OnTriggerLeave(ObjectReference akActionRef)
	If CurrentSpeaker != None && CurrentSpeaker.GetReference() == akActionRef
		CurrentSpeaker.Clear()
	EndIf
EndEvent
