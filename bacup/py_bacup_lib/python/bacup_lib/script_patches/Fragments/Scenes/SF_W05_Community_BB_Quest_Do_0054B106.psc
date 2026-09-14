Function Fragment_Phase_02_Begin()
	Quest owningQuest = GetOwningQuest()
	If owningQuest && owningQuest.IsStageDone(20) && !owningQuest.IsStageDone(40)
		owningQuest.SetStage(40)
	EndIf
EndFunction
