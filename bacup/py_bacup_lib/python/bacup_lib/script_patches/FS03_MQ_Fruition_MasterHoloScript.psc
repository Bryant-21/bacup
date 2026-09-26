; @drop-member OnHolotapePlay

Function HandleHolotapePlay(ObjectReference akTerminalRef)
	If akTerminalRef == None || FS03_MQ_Fruition == None
		Return
	EndIf

	FS03_MQ_Fruition_QuestScript questController = FS03_MQ_Fruition as FS03_MQ_Fruition_QuestScript
	If questController == None || !questController.IsRunning() || questController.GetStageDone(1000)
		Return
	EndIf

	; GetCurrentStageID is the highest stage done, and the reward stages 476/676 sit above the
	; load stages, so progress is gated on done-windows instead of the current stage.
	If akTerminalRef == FS03_MQ_Fruition_ArmoryTerminalRef && questController.GetStageDone(questController.iLoadHoloArmory) && !questController.GetStageDone(questController.iRunArmory)
		questController.SetStage(questController.iRunArmory)
	ElseIf akTerminalRef == FS03_MQ_Fruition_RaleighTerminalRef && questController.GetStageDone(questController.iLoadHoloRaleigh) && !questController.GetStageDone(questController.iDownloadSchematics)
		questController.SetStage(questController.iDownloadSchematics)
	ElseIf akTerminalRef == FS03_MQ_Fruition_SamTerminalRef && questController.GetStageDone(475) && !questController.GetStageDone(questController.iDownloadCodes)
		If !akTerminalRef.IsLocked()
			questController.SetStage(questController.iDownloadCodes)
		EndIf
	ElseIf FS03_MQ_Fruition_RelayTerminalKeyword != None && akTerminalRef.HasKeyword(FS03_MQ_Fruition_RelayTerminalKeyword) && questController.GetStageDone(questController.iLoadHoloRelay) && !questController.GetStageDone(questController.iUploadData)
		questController.SetStage(questController.iUploadData)
	EndIf
EndFunction

Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
	If FS03_MQ_Fruition == None || !FS03_MQ_Fruition.IsRunning() || akTerminalRef == None
		Return
	EndIf
	FS03_MQ_Fruition_QuestScript questController = FS03_MQ_Fruition as FS03_MQ_Fruition_QuestScript
	If questController == None || questController.GetStageDone(1000)
		Return
	EndIf
	If auiMenuItemID == 2 && akTerminalRef == FS03_MQ_Fruition_RaleighTerminalRef && questController.GetStageDone(questController.iDownloadSchematics) && !questController.GetStageDone(questController.iSchematicsDone)
		questController.SetStage(questController.iSchematicsDone)
	ElseIf auiMenuItemID == 3 && akTerminalRef == FS03_MQ_Fruition_SamTerminalRef && questController.GetStageDone(questController.iDownloadCodes) && !questController.GetStageDone(questController.iCodesDone)
		questController.SetStage(questController.iCodesDone)
	ElseIf auiMenuItemID == 4 && FS03_MQ_Fruition_RelayTerminalKeyword != None && akTerminalRef.HasKeyword(FS03_MQ_Fruition_RelayTerminalKeyword) && questController.GetStageDone(questController.iUploadData) && !questController.GetStageDone(800)
		questController.SetStage(800)
	EndIf
EndEvent
