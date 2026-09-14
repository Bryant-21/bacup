Function RecountUpgradedDetectors()
	DetectorCount = 0
	If GetStageDone(210)
		DetectorCount += 1
	EndIf
	If GetStageDone(220)
		DetectorCount += 1
	EndIf
	If GetStageDone(230)
		DetectorCount += 1
	EndIf
	If GetStageDone(240)
		DetectorCount += 1
	EndIf
	If GetStageDone(250)
		DetectorCount += 1
	EndIf
EndFunction

Function RestoreCheckpointInventory()
	Actor playerRef = FS02Player.GetActorReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef == None
		Return
	EndIf

	Int currentStage = GetCurrentStageID()
	If currentStage >= 200 && currentStage < 260
		RecountUpgradedDetectors()
		Int transmittersNeeded = 5 - DetectorCount
		Int transmitterShortfall = transmittersNeeded - playerRef.GetItemCount(FS01_MQ_Warn_UpgradedTransmitter)
		Int checkpointIndex = 0
		While transmitterShortfall > 0 && CPUpgradedTransmitters != None && checkpointIndex < CPUpgradedTransmitters.GetCount()
			ObjectReference checkpointItem = CPUpgradedTransmitters.GetAt(checkpointIndex)
			If checkpointItem != None && checkpointItem.GetContainer() != None && checkpointItem.GetContainer() != playerRef
				checkpointItem.GetContainer().RemoveItem(checkpointItem, 1, True, playerRef)
				transmitterShortfall -= 1
			EndIf
			checkpointIndex += 1
		EndWhile
		If transmitterShortfall > 0
			playerRef.AddItem(FS01_MQ_Warn_UpgradedTransmitter, transmitterShortfall, True)
		EndIf
	EndIf

	If currentStage < 410 && playerRef.GetItemCount(FS01_MQ_Warn_UplinkMiscItem) == 0
		ObjectReference checkpointUplink = CPUplink.GetReference()
		If checkpointUplink != None && checkpointUplink.GetContainer() != None && checkpointUplink.GetContainer() != playerRef
			checkpointUplink.GetContainer().RemoveItem(checkpointUplink, 1, True, playerRef)
		EndIf
		If playerRef.GetItemCount(FS01_MQ_Warn_UplinkMiscItem) == 0
			playerRef.AddItem(FS01_MQ_Warn_UplinkMiscItem, 1, True)
		EndIf
	EndIf
EndFunction

Event OnQuestInit()
	Parent.OnQuestInit()
	If FS01_MQ_Warn != None && !FS01_MQ_Warn.GetStageDone(1000)
		FS01_MQ_Warn.SetStage(1000)
	EndIf
	RecountUpgradedDetectors()
	RestoreCheckpointInventory()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID == 200 || (auiStageID >= 210 && auiStageID <= 260)
		RecountUpgradedDetectors()
		RestoreCheckpointInventory()
	ElseIf auiStageID == 410 && FS02_MQ_Reassembly_UplinkScene != None && !FS02_MQ_Reassembly_UplinkScene.IsPlaying()
		FS02_MQ_Reassembly_UplinkScene.Start()
	EndIf
EndEvent
