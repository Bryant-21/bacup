Event OnQuestInit()
	SetObjectiveDisplayed(110, True, True)
	SetObjectiveDisplayed(120, True, True)
	SetObjectiveDisplayed(130, True, True)
	If !IsStageDone(1)
		SetStage(1)
	EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID == 210 || auiStageID == 220 || auiStageID == 230
		SetObjectiveCompleted(100 + auiStageID - 200, True)
		If IsStageDone(210) && IsStageDone(220) && IsStageDone(230) && !IsStageDone(StageToSetWhenAllCollectStagesAreCompleted)
			SetStage(StageToSetWhenAllCollectStagesAreCompleted)
		EndIf
	ElseIf auiStageID == StageToSetWhenAllCollectStagesAreCompleted
		SetObjectiveDisplayed(ObjectiveWaitForUnveiling, True, True)
		StartTimer(TimeToWaitForUnveiling, 300)
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == 300 && IsRunning() && !IsStageDone(999)
		If CB05_Temp_UnveilingMessage != None
			CB05_Temp_UnveilingMessage.Show()
		EndIf
		SetObjectiveCompleted(ObjectiveWaitForUnveiling, True)
		SetStage(999)
		; 999 only carries the CompleteQuest flag; nothing else in FO4 shuts the quest down.
		Stop()
	EndIf
EndEvent
