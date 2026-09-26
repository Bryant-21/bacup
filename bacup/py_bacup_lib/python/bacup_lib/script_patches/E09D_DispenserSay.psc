Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
	If akSenderRef == None || dispenserTopic == None || akActionRef != Game.GetPlayer()
		Return
	EndIf
	akSenderRef.Say(dispenserTopic, None, False)
EndEvent
