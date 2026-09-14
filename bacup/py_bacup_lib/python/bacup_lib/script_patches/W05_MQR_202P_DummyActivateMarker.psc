Event OnActivate(ObjectReference akActionRef)
	ObjectReference nextMarker = GetLinkedRef()
	If nextMarker != None
		nextMarker.Activate(akActionRef)
	EndIf
EndEvent
