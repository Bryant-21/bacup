Event OnActivate(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer() && KeyworkToSend != None && (BlockingKeyword == None || !akActionRef.HasKeyword(BlockingKeyword))
		KeyworkToSend.SendStoryEventAndWait(akActionRef.GetCurrentLocation(), akActionRef, GetReference())
	EndIf
EndEvent
