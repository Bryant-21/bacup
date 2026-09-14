Function RefreshClueProgress()
	clueCounter = 0
	Int clueStage = minClueStage
	While clueStage <= maxClueStage
		If IsStageDone(clueStage)
			clueCounter += 1
		EndIf
		clueStage += 1
	EndWhile

	If IsObjectiveDisplayed(52)
		SetObjectiveDisplayed(52, True, True)
	EndIf
	If clueCounter >= totalCluesNeeded && !IsStageDone(allCluesFoundStage)
		SetStage(allCluesFoundStage)
	EndIf
EndFunction

Event OnQuestInit()
	RefreshClueProgress()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID >= minClueStage && auiStageID <= maxClueStage
		RefreshClueProgress()
	EndIf
EndEvent
