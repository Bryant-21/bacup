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
	Quest owningQuest = GetOwningQuest()
	If akActionRef == Game.GetPlayer() && owningQuest != None && StageToSet >= 0 && (PrereqStage < 0 || owningQuest.IsStageDone(PrereqStage))
		owningQuest.SetStage(StageToSet)
	EndIf
EndEvent
