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
	Actor playerRef = Alias_FS02Player.GetActorReference()
	If playerRef != None
		playerRef.SetValue(FS03_CheckpointValue, 25.0)
	EndIf
	; Reassembly Required hands off on entering Abbie's bunker, so the player can already be past
	; the small AbbieTriggerVolume (alias 37) when this quest fills it; its OnTriggerEnter never fires.
	ReferenceAlias triggerAlias = GetAlias(37) as ReferenceAlias
	If playerRef != None && triggerAlias != None && !GetStageDone(50)
		ObjectReference triggerRef = triggerAlias.GetReference()
		If triggerRef != None && triggerRef.GetCurrentLocation() != None && playerRef.GetCurrentLocation() == triggerRef.GetCurrentLocation()
			SetStage(50)
		EndIf
	EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
	SetObjectiveDisplayed(50, True, True)
	If FS03_MQ_Fruition_AbbieIntroScene != None && !FS03_MQ_Fruition_AbbieIntroScene.IsPlaying()
		FS03_MQ_Fruition_AbbieIntroScene.Start()
	EndIf
	FS03_ArmSceneHandoff(50, 0)
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveCompleted(50, True)
	SetObjectiveDisplayed(100, True, True)
EndFunction

Function Fragment_Stage_0110_Item_00()
	Actor playerRef = Alias_FS02Player.GetActorReference()
	If playerRef != None
		playerRef.SetValue(FS03_CheckpointValue, 275.0)
	EndIf
	SetObjectiveCompleted(100, True)
	SetObjectiveDisplayed(275, True, True)
	; 275 is the armory load stage: the master holotape watcher and alias 38 both key on it.
	If !GetStageDone(275)
		SetStage(275)
	EndIf
EndFunction

Function Fragment_Stage_0275_Item_00()
	SetObjectiveDisplayed(275, True, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(275, True)
	SetObjectiveDisplayed(300, True, True)
EndFunction

Function Fragment_Stage_0350_Item_00()
	SetObjectiveCompleted(300, True)
	SetObjectiveDisplayed(350, True, True)
	If FS02_Fruition_AbbieArmoryEntranceScene != None && !FS02_Fruition_AbbieArmoryEntranceScene.IsPlaying()
		FS02_Fruition_AbbieArmoryEntranceScene.Start()
	EndIf
	FS03_ArmSceneHandoff(350, 0)
EndFunction

Function Fragment_Stage_0375_Item_00()
	Bool usedAlternateEntrance = !GetStageDone(350)
	SetObjectiveCompleted(300, True)
	SetObjectiveCompleted(350, True)
	SetObjectiveDisplayed(375, True, True)
	If usedAlternateEntrance && FS02_Fruition_AbbieArmoryEntranceAltScene != None && !FS02_Fruition_AbbieArmoryEntranceAltScene.IsPlaying()
		FS02_Fruition_AbbieArmoryEntranceAltScene.Start()
	EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(375, True)
	SetObjectiveDisplayed(400, True, True)
EndFunction

Function Fragment_Stage_0425_Item_00()
	SetObjectiveCompleted(400, True)
	If !GetStageDone(450)
		SetStage(450)
	EndIf
EndFunction

Function Fragment_Stage_0450_Item_00()
	SetObjectiveDisplayed(450, True, True)
	If FS02_Fruition_AbbieArmoryTerminalScene != None && !FS02_Fruition_AbbieArmoryTerminalScene.IsPlaying()
		FS02_Fruition_AbbieArmoryTerminalScene.Start()
	EndIf
	FS03_ArmSceneHandoff(450, 0)
EndFunction

Function Fragment_Stage_0475_Item_00()
	Actor playerRef = Alias_FS02Player.GetActorReference()
	If playerRef != None
		playerRef.SetValue(FS03_CheckpointValue, 475.0)
	EndIf
	SetObjectiveCompleted(450, True)
	SetObjectiveDisplayed(475, True, True)
	If !GetStageDone(476)
		SetStage(476)
	EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveDisplayed(475, False)
	SetObjectiveDisplayed(500, True, True)
EndFunction

Function Fragment_Stage_0525_Item_00()
	SetObjectiveCompleted(500, True)
	SetObjectiveDisplayed(475, True, True)
EndFunction

Function Fragment_Stage_0600_Item_00()
	SetObjectiveCompleted(475, True)
	SetObjectiveCompleted(500, True)
	SetObjectiveDisplayed(600, True, True)
EndFunction

Function Fragment_Stage_0625_Item_00()
	Actor playerRef = Alias_FS02Player.GetActorReference()
	If playerRef != None && FS02_Fruition_NirajPassword != None && playerRef.GetItemCount(FS02_Fruition_NirajPassword) == 0
		playerRef.AddItem(FS02_Fruition_NirajPassword, 1, True)
	EndIf
	SetObjectiveCompleted(600, True)
	If !GetStageDone(650)
		SetStage(650)
	EndIf
EndFunction

Function Fragment_Stage_0650_Item_00()
	SetObjectiveDisplayed(650, True, True)
	If FS03_MQ_Fruition_AbbieSamTerminalScene != None && !FS03_MQ_Fruition_AbbieSamTerminalScene.IsPlaying()
		FS03_MQ_Fruition_AbbieSamTerminalScene.Start()
	EndIf
	FS03_ArmSceneHandoff(650, 0)
EndFunction

Function Fragment_Stage_0660_Item_00()
	SetObjectiveCompleted(650, True)
	If !GetStageDone(675)
		SetStage(675)
	EndIf
EndFunction

Function Fragment_Stage_0675_Item_00()
	Actor playerRef = Alias_FS02Player.GetActorReference()
	If playerRef != None
		playerRef.SetValue(FS03_CheckpointValue, 675.0)
	EndIf
	SetObjectiveDisplayed(675, True, True)
	If !GetStageDone(676)
		SetStage(676)
	EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
	SetObjectiveCompleted(675, True)
	SetObjectiveDisplayed(700, True, True)
EndFunction

Function Fragment_Stage_0800_Item_00()
	Actor playerRef = Alias_FS02Player.GetActorReference()
	If playerRef != None
		playerRef.SetValue(FS03_CheckpointValue, 800.0)
		If FS02_Fruition_NirajPassword != None && playerRef.GetItemCount(FS02_Fruition_NirajPassword) > 0
			playerRef.RemoveItem(FS02_Fruition_NirajPassword, 1, True)
		EndIf
	EndIf
	SetObjectiveCompleted(700, True)
	SetObjectiveDisplayed(800, True, True)
	If !GetStageDone(801)
		SetStage(801)
	EndIf
EndFunction

Function Fragment_Stage_0850_Item_00()
	SetObjectiveCompleted(800, True)
	SetObjectiveDisplayed(850, True, True)
	ObjectReference terminalRef = Alias_SDSTerminal.GetReference()
	If QSTFS03SystemReboot != None && terminalRef != None
		QSTFS03SystemReboot.Play(terminalRef)
	EndIf
	If FS02_Fruition_AbbieFinishScene != None && !FS02_Fruition_AbbieFinishScene.IsPlaying()
		FS02_Fruition_AbbieFinishScene.Start()
	EndIf
	FS03_ArmSceneHandoff(850, 0)
EndFunction

Function Fragment_Stage_0900_Item_00()
	SetObjectiveCompleted(850, True)
	If FS03_TryStartBoS01()
		If !GetStageDone(1000)
			SetStage(1000)
		EndIf
	Else
		StartTimer(5.0, 900)
	EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
	Actor playerRef = Alias_FS02Player.GetActorReference()
	If playerRef != None && FS02_Fruition_QuestCompletedValue != None
		playerRef.SetValue(FS02_Fruition_QuestCompletedValue, 1.0)
	EndIf
	SetObjectiveCompleted(850, True)
	CompleteQuest()
	Stop()
EndFunction

Bool Function FS03_TryStartBoS01()
	If BoS01 != None && (BoS01.IsRunning() || BoS01.IsCompleted())
		Return True
	EndIf

	Actor playerRef = Alias_FS02Player.GetActorReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef == None || BoS01_QuestStartKeyword == None
		Return False
	EndIf

	Bool accepted = BoS01_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
	Return accepted || (BoS01 != None && (BoS01.IsRunning() || BoS01.IsCompleted()))
EndFunction

Function FS03_ArmSceneHandoff(Int aiSceneStage, Int aiAttempt)
	StartTimer(15.0, 100000 + aiSceneStage * 100 + aiAttempt)
EndFunction

Function FS03_CheckSceneHandoff(Int aiSceneStage, Int aiAttempt)
	; Each Abbie scene advances the quest only through its last INFO's end stage. Three of them
	; speak through FO76 terminal refs, which FO4 cannot voice, so a stalled or failed scene
	; would park the quest; wait while it plays (bounded), then set the stage it would have set.
	Scene handoffScene = None
	Int nextStage = -1
	If aiSceneStage == 50
		handoffScene = FS03_MQ_Fruition_AbbieIntroScene
		nextStage = 100
	ElseIf aiSceneStage == 350
		handoffScene = FS02_Fruition_AbbieArmoryEntranceScene
		nextStage = 375
	ElseIf aiSceneStage == 450
		handoffScene = FS02_Fruition_AbbieArmoryTerminalScene
		nextStage = 475
	ElseIf aiSceneStage == 650
		handoffScene = FS03_MQ_Fruition_AbbieSamTerminalScene
		nextStage = 660
	ElseIf aiSceneStage == 850
		handoffScene = FS02_Fruition_AbbieFinishScene
		nextStage = 900
	EndIf
	If nextStage < 0 || !IsRunning() || GetStageDone(nextStage)
		Return
	EndIf
	If handoffScene != None && handoffScene.IsPlaying() && aiAttempt < 12
		FS03_ArmSceneHandoff(aiSceneStage, aiAttempt + 1)
		Return
	EndIf
	SetStage(nextStage)
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID >= 100000
		FS03_CheckSceneHandoff((aiTimerID - 100000) / 100, aiTimerID % 100)
		Return
	EndIf
	If aiTimerID != 900 || !GetStageDone(900) || GetStageDone(1000)
		Return
	EndIf

	If FS03_TryStartBoS01()
		SetStage(1000)
	Else
		StartTimer(5.0, 900)
	EndIf
EndEvent
