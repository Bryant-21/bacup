; Stage fragments for Powering Up (PowerPlantEvent 3E4E89). The quest's own state machine lives in
; PowerPlantEventQuestScript; these fragments only do the stage-local objective, log and shutdown
; work, exactly as the stage notes describe:
;   20 [LOG] Repair, Below Threshold      30 [LOG] Repair, Above Threshold
;   31 [LOG] Repair, Fully Repaired       35 Update Repair Objectives
;   40 Restart, Not Permitted             41 Restart, Permitted
;   100 [LOG] Completed                   250 Event Failed (stage flag FailQuest)
; Stage 10 (RunOnStart) and 255 (RunOnStop) carry no fragment; the root script handles both.
;
; P01C_Tadpole_RestorePowerPlant is a CHAL record with no FO4 equivalent and converts to a null
; FormID, so no fragment touches it.

PowerPlantEventQuestScript Function GetEventScript()
	Return (Self as Quest) as PowerPlantEventQuestScript
EndFunction

Function FailObjectiveIfOpen(Int aiObjectiveIndex)
	If IsObjectiveDisplayed(aiObjectiveIndex) && !IsObjectiveCompleted(aiObjectiveIndex)
		SetObjectiveFailed(aiObjectiveIndex, True)
	EndIf
EndFunction

Function CompleteObjectiveIfOpen(Int aiObjectiveIndex)
	If IsObjectiveDisplayed(aiObjectiveIndex) && !IsObjectiveCompleted(aiObjectiveIndex)
		SetObjectiveCompleted(aiObjectiveIndex, True)
	EndIf
EndFunction

Function ShutdownEvent(Bool abFailed)
	If abFailed
		FailObjectiveIfOpen(20)
		FailObjectiveIfOpen(21)
		FailObjectiveIfOpen(22)
		FailObjectiveIfOpen(23)
		FailObjectiveIfOpen(30)
		FailObjectiveIfOpen(31)
		FailObjectiveIfOpen(32)
		FailObjectiveIfOpen(33)
		FailObjectiveIfOpen(40)
	EndIf
	Stop()
EndFunction

Function RefreshRepairObjectives()
	PowerPlantEventQuestScript eventScript = GetEventScript()
	If eventScript != None
		eventScript.RefreshRepairObjectives()
	EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
	RefreshRepairObjectives()
	If !IsStageDone(40)
		SetStage(40)
	EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
	RefreshRepairObjectives()
	If !IsStageDone(41)
		SetStage(41)
	EndIf
EndFunction

Function Fragment_Stage_0031_Item_00()
	RefreshRepairObjectives()
	If !IsStageDone(41)
		SetStage(41)
	EndIf
EndFunction

Function Fragment_Stage_0035_Item_00()
	RefreshRepairObjectives()
EndFunction

Function Fragment_Stage_0040_Item_00()
	SetObjectiveDisplayed(40, False)
	PowerPlantEventQuestScript eventScript = GetEventScript()
	If eventScript != None
		eventScript.ApplyTerminalRepairNeeded(True)
	EndIf
EndFunction

Function Fragment_Stage_0041_Item_00()
	PowerPlantEventQuestScript eventScript = GetEventScript()
	If eventScript != None
		eventScript.ApplyTerminalReadyForRestart()
	EndIf
	SetObjectiveDisplayed(40, True, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
	CompleteObjectiveIfOpen(20)
	CompleteObjectiveIfOpen(21)
	CompleteObjectiveIfOpen(22)
	CompleteObjectiveIfOpen(23)
	CompleteObjectiveIfOpen(30)
	CompleteObjectiveIfOpen(31)
	CompleteObjectiveIfOpen(32)
	CompleteObjectiveIfOpen(33)
	SetObjectiveDisplayed(40, True)
	SetObjectiveCompleted(40, True)
	PowerPlantEventQuestScript eventScript = GetEventScript()
	If eventScript != None
		eventScript.FinishPowerPlantRestart()
	EndIf
	CompleteQuest()
EndFunction

Function Fragment_Stage_0250_Item_00()
	ShutdownEvent(True)
EndFunction
