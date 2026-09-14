Event OnAliasInit()
	Int index = 0
	While index < GetCount()
		ObjectReference triggerRef = GetAt(index)
		If triggerRef != None
			RegisterForRemoteEvent(triggerRef, "OnTriggerEnter")
		EndIf
		index += 1
	EndWhile
EndEvent

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer() && CurrentTarget != None
		ObjectReference nextTarget = akSender.GetLinkedRef()
		If nextTarget != None
			CurrentTarget.ForceRefTo(nextTarget)
		EndIf
	EndIf
EndEvent
