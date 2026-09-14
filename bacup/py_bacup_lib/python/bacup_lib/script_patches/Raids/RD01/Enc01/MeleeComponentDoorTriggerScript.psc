Event OnAliasInit()
	Int index = 0
	While index < GetCount()
		ObjectReference triggerRef = GetAt(index)
		If triggerRef != None
			RegisterForRemoteEvent(triggerRef, "OnTriggerEnter")
			RegisterForRemoteEvent(triggerRef, "OnTriggerLeave")
		EndIf
		index += 1
	EndWhile
EndEvent

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer()
		ObjectReference doorRef = akSender.GetLinkedRef()
		If doorRef != None
			doorRef.SetOpen(True)
		EndIf
	EndIf
EndEvent

Event ObjectReference.OnTriggerLeave(ObjectReference akSender, ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer()
		ObjectReference doorRef = akSender.GetLinkedRef()
		If doorRef != None
			doorRef.SetOpen(False)
		EndIf
	EndIf
EndEvent
