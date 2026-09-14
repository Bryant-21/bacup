Event OnHolotapePlay(ObjectReference akTerminalRef)
	If akTerminalRef == None || FS03_MQ_Fruition == None
		Return
	EndIf

	FS03_MQ_Fruition_QuestScript questController = FS03_MQ_Fruition as FS03_MQ_Fruition_QuestScript
	If questController == None || questController.GetStageDone(1000)
		Return
	EndIf

	Int currentStage = questController.GetCurrentStageID()
	If akTerminalRef == FS03_MQ_Fruition_ArmoryTerminalRef && currentStage == questController.iLoadHoloArmory
		questController.SetStage(questController.iRunArmory)
	ElseIf akTerminalRef == FS03_MQ_Fruition_RaleighTerminalRef && currentStage == questController.iLoadHoloRaleigh
		questController.SetStage(questController.iDownloadSchematics)
	ElseIf akTerminalRef == FS03_MQ_Fruition_SamTerminalRef && (currentStage == 475 || currentStage == questController.iSamUnlocked)
		If !akTerminalRef.IsLocked()
			questController.SetStage(questController.iDownloadCodes)
		EndIf
	ElseIf FS03_MQ_Fruition_RelayTerminalKeyword != None && akTerminalRef.HasKeyword(FS03_MQ_Fruition_RelayTerminalKeyword) && currentStage == questController.iLoadHoloRelay
		questController.SetStage(questController.iUploadData)
	EndIf
EndEvent
