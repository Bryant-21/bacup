Function Fragment_Stage_0000_Item_00()
	If !GetStageDone(10)
		SetStage(10)
	EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
	If !GetStageDone(25)
		SetStage(25)
	EndIf
EndFunction

Function Fragment_Stage_0025_Item_00()
	SetObjectiveDisplayed(50, True, True)
EndFunction

Function Fragment_Stage_0050_Item_00()
	SetObjectiveDisplayed(50, True, True)
	If FS02_MQ_Reassembly_AbbieIntroScene != None && !FS02_MQ_Reassembly_AbbieIntroScene.IsPlaying()
		FS02_MQ_Reassembly_AbbieIntroScene.Start()
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveCompleted(50, True)
	SetObjectiveDisplayed(100, True, True)
EndFunction

Function Fragment_Stage_0150_Item_00()
	SetObjectiveCompleted(100, True)
	If !GetStageDone(200)
		SetStage(200)
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	Actor playerRef = Alias_FS02Player.GetActorReference()
	If playerRef != None
		playerRef.SetValue(FS02_CheckpointValue, 200.0)
	EndIf
	SetObjectiveDisplayed(200, True, True)
EndFunction

Function Fragment_Stage_0205_Item_00()
	SetObjectiveDisplayed(200, True, True)
EndFunction

Function Fragment_Stage_0210_Item_00()
	Actor playerRef = Alias_FS02Player.GetActorReference()
	If playerRef != None
		playerRef.SetValue(FS02_CheckpointDetector01, 1.0)
	EndIf
	If QSTSFL02SignalBoostInstall != None && Alias_ScorchedDetector01.GetReference() != None
		QSTSFL02SignalBoostInstall.Play(Alias_ScorchedDetector01.GetReference())
	EndIf
	SetObjectiveDisplayed(200, True, True)
	If GetStageDone(220) && GetStageDone(230) && GetStageDone(240) && GetStageDone(250) && !GetStageDone(260)
		SetStage(260)
	EndIf
EndFunction

Function Fragment_Stage_0220_Item_00()
	Actor playerRef = Alias_FS02Player.GetActorReference()
	If playerRef != None
		playerRef.SetValue(FS02_CheckpointDetector02, 1.0)
	EndIf
	If QSTSFL02SignalBoostInstall != None && Alias_ScorchedDetector02.GetReference() != None
		QSTSFL02SignalBoostInstall.Play(Alias_ScorchedDetector02.GetReference())
	EndIf
	SetObjectiveDisplayed(200, True, True)
	If GetStageDone(210) && GetStageDone(230) && GetStageDone(240) && GetStageDone(250) && !GetStageDone(260)
		SetStage(260)
	EndIf
EndFunction

Function Fragment_Stage_0230_Item_00()
	Actor playerRef = Alias_FS02Player.GetActorReference()
	If playerRef != None
		playerRef.SetValue(FS02_CheckpointDetector03, 1.0)
	EndIf
	If QSTSFL02SignalBoostInstall != None && Alias_ScorchedDetector03.GetReference() != None
		QSTSFL02SignalBoostInstall.Play(Alias_ScorchedDetector03.GetReference())
	EndIf
	SetObjectiveDisplayed(200, True, True)
	If GetStageDone(210) && GetStageDone(220) && GetStageDone(240) && GetStageDone(250) && !GetStageDone(260)
		SetStage(260)
	EndIf
EndFunction

Function Fragment_Stage_0240_Item_00()
	Actor playerRef = Alias_FS02Player.GetActorReference()
	If playerRef != None
		playerRef.SetValue(FS02_CheckpointDetector04, 1.0)
	EndIf
	If QSTSFL02SignalBoostInstall != None && Alias_ScorchedDetector04.GetReference() != None
		QSTSFL02SignalBoostInstall.Play(Alias_ScorchedDetector04.GetReference())
	EndIf
	SetObjectiveDisplayed(200, True, True)
	If GetStageDone(210) && GetStageDone(220) && GetStageDone(230) && GetStageDone(250) && !GetStageDone(260)
		SetStage(260)
	EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
	Actor playerRef = Alias_FS02Player.GetActorReference()
	If playerRef != None
		playerRef.SetValue(FS02_CheckpointDetector05, 1.0)
	EndIf
	If QSTSFL02SignalBoostInstall != None && Alias_ScorchedDetector05.GetReference() != None
		QSTSFL02SignalBoostInstall.Play(Alias_ScorchedDetector05.GetReference())
	EndIf
	SetObjectiveDisplayed(200, True, True)
	If GetStageDone(210) && GetStageDone(220) && GetStageDone(230) && GetStageDone(240) && !GetStageDone(260)
		SetStage(260)
	EndIf
EndFunction

Function Fragment_Stage_0260_Item_00()
	Actor playerRef = Alias_FS02Player.GetActorReference()
	If playerRef != None
		playerRef.SetValue(FS02_CheckpointValue, 300.0)
	EndIf
	SetObjectiveCompleted(200, True)
	If !GetStageDone(300)
		SetStage(300)
	EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(200, True)
	SetObjectiveDisplayed(300, True, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(300, True)
	SetObjectiveDisplayed(400, True, True)
	If FS02_MQ_Overdue_RoseScene != None && !FS02_MQ_Overdue_RoseScene.IsPlaying()
		FS02_MQ_Overdue_RoseScene.Start()
	EndIf
EndFunction

Function Fragment_Stage_0410_Item_00()
	Actor playerRef = Alias_FS02Player.GetActorReference()
	If playerRef != None && playerRef.GetItemCount(FS01_MQ_Warn_UplinkMiscItem) > 0
		playerRef.RemoveItem(FS01_MQ_Warn_UplinkMiscItem, 1, True)
	EndIf
	If Alias_Uplink != None
		Alias_Uplink.Clear()
	EndIf
EndFunction

Function Fragment_Stage_0450_Item_00()
	If !GetStageDone(500)
		SetStage(500)
	EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
	Actor playerRef = Alias_FS02Player.GetActorReference()
	If playerRef != None
		playerRef.SetValue(FS02_CheckpointValue, 500.0)
	EndIf
	SetObjectiveCompleted(400, True)
	SetObjectiveDisplayed(500, True, True)
EndFunction

Function Fragment_Stage_0600_Item_00()
	SetObjectiveCompleted(500, True)
	If FS02_TryStartFruition()
		If !GetStageDone(1000)
			SetStage(1000)
		EndIf
	Else
		StartTimer(5.0, 600)
	EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
	SetObjectiveCompleted(500, True)
	CompleteQuest()
	Stop()
EndFunction

Bool Function FS02_TryStartFruition()
	Quest fruitionQuest = Game.GetFormFromFile(0x00012566, "SeventySix.esm") as Quest
	If fruitionQuest != None && (fruitionQuest.IsRunning() || fruitionQuest.IsCompleted())
		Return True
	EndIf

	ObjectReference playerRef = Alias_FS02Player.GetReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef == None || FS03_MQ_Fruition_QuestStartKeyword == None
		Return False
	EndIf

	Bool accepted = FS03_MQ_Fruition_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
	Return accepted || (fruitionQuest != None && (fruitionQuest.IsRunning() || fruitionQuest.IsCompleted()))
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID != 600 || !GetStageDone(600) || GetStageDone(1000)
		Return
	EndIf

	If FS02_TryStartFruition()
		SetStage(1000)
	Else
		StartTimer(5.0, 600)
	EndIf
EndEvent
