Event OnActivate(ObjectReference akActionRef)
	Quest owningQuest = GetOwningQuest()
	If owningQuest != None && !owningQuest.IsStageDone(200)
		owningQuest.SetStage(200)
	EndIf
EndEvent
