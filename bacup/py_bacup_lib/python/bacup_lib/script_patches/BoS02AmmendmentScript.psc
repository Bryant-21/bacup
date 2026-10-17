Event OnActivate(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer() as ObjectReference && pQSTBoS02AmendmentScribble
		PlayFillFormSound()
	EndIf
EndEvent
