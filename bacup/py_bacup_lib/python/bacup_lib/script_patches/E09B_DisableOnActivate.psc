Event OnActivate(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer()
		ObjectReference aliasRef = GetReference()
		If aliasRef != None
			aliasRef.Disable()
		EndIf
	EndIf
EndEvent
