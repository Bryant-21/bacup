Event OnActivate(ObjectReference akActionRef)
	If akActionRef != Game.GetPlayer()
		Return
	EndIf
	ActivateLinkedTarget(LinkCustom01, akActionRef)
	ActivateLinkedTarget(LinkCustom02, akActionRef)
	ActivateLinkedTarget(LinkCustom03, akActionRef)
	ActivateLinkedTarget(LinkCustom04, akActionRef)
	ActivateLinkedTarget(LinkCustom05, akActionRef)
	ActivateLinkedTarget(LinkCustom06, akActionRef)
	ActivateLinkedTarget(LinkCustom07, akActionRef)
	ActivateLinkedTarget(LinkCustom08, akActionRef)
EndEvent

Function ActivateLinkedTarget(Keyword linkKeyword, ObjectReference activatorRef)
	ObjectReference targetRef = GetLinkedRef(linkKeyword)
	If targetRef != None
		targetRef.Activate(activatorRef)
	EndIf
EndFunction
