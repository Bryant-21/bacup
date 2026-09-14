Event OnActivate(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer()
		ObjectReference chairRef = GetLinkedRef()
		If chairRef != None
			chairRef.Activate(akActionRef)
		EndIf
	EndIf
EndEvent
