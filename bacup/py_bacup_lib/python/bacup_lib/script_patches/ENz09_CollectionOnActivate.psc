Event OnAliasInit()
	Int index = 0
	While index < GetCount()
		ObjectReference collectionRef = GetAt(index)
		If collectionRef != None
			RegisterForRemoteEvent(collectionRef, "OnActivate")
		EndIf
		index += 1
	EndWhile
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
	Quest owningQuest = GetOwningQuest()
	If akActionRef == Game.GetPlayer() && owningQuest != None && (iPreReqStage < 0 || owningQuest.IsStageDone(iPreReqStage)) && iStageToSet >= 0
		owningQuest.SetStage(iStageToSet)
	EndIf
EndEvent
