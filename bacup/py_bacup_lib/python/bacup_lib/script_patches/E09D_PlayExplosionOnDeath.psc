Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
	If akSenderRef != None && FX != None
		akSenderRef.PlaceAtMe(FX, 1, False, False, True)
	EndIf
EndEvent
