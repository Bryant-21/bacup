Event OnActivate(ObjectReference akActionRef)
	If akActionRef != Game.GetPlayer()
		Return
	EndIf
	ActivateLinkedTarget(LinkCustom01, akActionRef)
	ActivateLinkedTarget(LinkCustom02, akActionRef)
	ActivateLinkedTarget(LinkCustom03, akActionRef)
EndEvent

Function ActivateLinkedTarget(Keyword linkKeyword, ObjectReference activatorRef)
	ObjectReference targetRef = GetLinkedRef(linkKeyword)
	If targetRef != None
		targetRef.Activate(activatorRef)
	EndIf
EndFunction
