Function Fragment_End()
	Quest owningQuest = GetOwningQuest()
	If owningQuest != None && owningQuest.GetStage() >= 1000 && !owningQuest.IsStageDone(1050)
		owningQuest.SetStage(1050)
	EndIf
EndFunction
