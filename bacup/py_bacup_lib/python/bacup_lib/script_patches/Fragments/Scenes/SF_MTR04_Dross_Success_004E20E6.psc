Function Fragment_Phase_01_End()
	Quest owningQuest = GetOwningQuest()
	If owningQuest != None
		owningQuest.Stop()
	EndIf
EndFunction
