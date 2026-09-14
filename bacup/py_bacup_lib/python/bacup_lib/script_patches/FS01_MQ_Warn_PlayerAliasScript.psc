Function ApplyNoCraftingProgress()
	Quest owningQuest = GetOwningQuest()
	If owningQuest == None || owningQuest.GetStageDone(1000)
		Return
	EndIf

	Int currentStage = owningQuest.GetCurrentStageID()
	If currentStage == 150 && !owningQuest.GetStageDone(200)
		owningQuest.SetStage(200)
	ElseIf currentStage >= 600 && currentStage < 700 && !owningQuest.GetStageDone(700)
		owningQuest.SetStage(700)
	EndIf
EndFunction

Event OnAliasInit()
	ApplyNoCraftingProgress()
EndEvent

Event OnPlayerLoadGame()
	ApplyNoCraftingProgress()
EndEvent
