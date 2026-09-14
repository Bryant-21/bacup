Event OnActivate(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer()
		ObjectReference targetRef = GetLinkedRef()
		If targetRef != None
			targetRef.Activate(akActionRef)
		EndIf
	EndIf
EndEvent
