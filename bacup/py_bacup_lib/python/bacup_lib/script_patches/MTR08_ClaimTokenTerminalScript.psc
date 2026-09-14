Event OnActivate(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer() && MTR08_ClaimToken != None
		Confirmation = akActionRef.GetItemCount(MTR08_ClaimToken) > 0
	EndIf
EndEvent
