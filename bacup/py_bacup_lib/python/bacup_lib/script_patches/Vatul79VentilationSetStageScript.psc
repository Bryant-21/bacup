Event OnActivate(ObjectReference akActionRef)
	Quest owningQuest = GetOwningQuest()
	If owningQuest != None && !owningQuest.IsStageDone(100)
		owningQuest.SetStage(100)
	EndIf
EndEvent
