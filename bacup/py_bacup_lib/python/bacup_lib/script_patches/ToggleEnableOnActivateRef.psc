Event OnActivate(ObjectReference akActionRef)
	If akActionRef != Game.GetPlayer()
		Return
	EndIf
	ObjectReference targetRef = GetLinkedRef()
	If targetRef != None
		If targetRef.IsDisabled()
			targetRef.Enable()
		Else
			targetRef.Disable()
		EndIf
	EndIf
EndEvent
