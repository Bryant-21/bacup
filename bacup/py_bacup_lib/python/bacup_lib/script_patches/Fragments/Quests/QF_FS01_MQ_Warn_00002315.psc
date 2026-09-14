Actor Function FS01_GetPlayer()
	If Alias_FS01Player == None
		Return None
	EndIf
	Return Alias_FS01Player.GetActorReference()
EndFunction

Function FS01_SetPlayerValue(ActorValue akValue, Float afValue)
	Actor playerRef = FS01_GetPlayer()
	If playerRef != None && akValue != None
		playerRef.SetValue(akValue, afValue)
	EndIf
EndFunction

Function FS01_GiveItem(Form akItem, Int aiCount = 1)
	Actor playerRef = FS01_GetPlayer()
	If playerRef != None && akItem != None && aiCount > 0 && playerRef.GetItemCount(akItem) < aiCount
		playerRef.AddItem(akItem, aiCount - playerRef.GetItemCount(akItem), True)
	EndIf
EndFunction

Function FS01_StartScene(Scene akScene)
	If akScene != None && !akScene.IsPlaying()
		akScene.Start()
	EndIf
EndFunction

Function FS01_RunOnStart()
	Actor playerRef = FS01_GetPlayer()
	If playerRef != None
		FS01_SetPlayerValue(FS01_MQ_Warn_StartedValue, 1.0)

		If FS01_Warn_UplinkMiscItem != None && playerRef.GetItemCount(FS01_Warn_UplinkMiscItem) > 0
			If !GetStageDone(200)
				SetStage(200)
			EndIf
			Return
		EndIf

		If FS01_MQ_Warn_BrokenUplinkMiscItem != None && playerRef.GetItemCount(FS01_MQ_Warn_BrokenUplinkMiscItem) > 0
			If !GetStageDone(150)
				SetStage(150)
			EndIf
			Return
		EndIf

		Quest missingLink = Game.GetFormFromFile(0x003A5FA0, "SeventySix.esm") as Quest
		If missingLink != None && !missingLink.IsRunning() && !missingLink.GetStageDone(500) && MTN_MQ_Missing_Quest_Keyword != None
			MTN_MQ_Missing_Quest_Keyword.SendStoryEventAndWait(None, playerRef, playerRef)
		EndIf
	EndIf

	If !GetStageDone(25)
		SetStage(25)
	EndIf
EndFunction

Function FS01_GiveRaleighPassword()
	FS01_GiveItem(FS01_Hope_RaleighPassword, 1)
	FS01_SetPlayerValue(FS01_CheckpointRaleighPassword, 1.0)
EndFunction

Function FS01_SetMajorCheckpoint(Int aiCheckpoint)
	FS01_SetPlayerValue(FS01_CheckpointValue, aiCheckpoint as Float)
EndFunction

Function FS01_EnterAmplifierSearch(Bool abShowSchematics)
	FS01_SetMajorCheckpoint(500)
	SetObjectiveCompleted(400, True)
	SetObjectiveDisplayed(500, True, True)
	SetObjectiveDisplayed(501, True, True)
	SetObjectiveDisplayed(502, True, True)
	SetObjectiveDisplayed(503, True, True)
	If abShowSchematics && !GetStageDone(455)
		SetObjectiveDisplayed(450, True, True)
	EndIf

	If RaleighBunkerMapMarkerREF != None
		RaleighBunkerMapMarkerREF.AddToMap(False)
	EndIf
	If EllaAmesBunkerMapMarkerREF != None
		EllaAmesBunkerMapMarkerREF.AddToMap(False)
	EndIf
	If RelayTower04MapMarkerRef != None
		RelayTower04MapMarkerRef.AddToMap(False)
	EndIf

	If !GetStageDone(501)
		SetStage(501)
	EndIf
EndFunction

Function FS01_UpdateAbbieHeatingCoils()
	AbbieHeatingCoilCount = 0
	If GetStageDone(510)
		AbbieHeatingCoilCount += 1
	EndIf
	If GetStageDone(520)
		AbbieHeatingCoilCount += 1
	EndIf
	If AbbieHeatingCoilCount >= 2
		SetObjectiveCompleted(503, True)
	Else
		SetObjectiveDisplayed(503, True, True)
	EndIf
EndFunction

Function FS01_CheckAmplifiersComplete()
	If GetStageDone(510) && GetStageDone(520) && GetStageDone(530) && GetStageDone(540) && GetStageDone(550) && !GetStageDone(560)
		SetStage(560)
	EndIf
EndFunction

Function Fragment_Stage_0000_Item_00()
	Actor playerRef = FS01_GetPlayer()
	If playerRef != None
		If c_Steel_scrap != None
			playerRef.AddItem(c_Steel_scrap, 20, True)
		EndIf
		If c_Circuitry_scrap != None
			playerRef.AddItem(c_Circuitry_scrap, 3, True)
		EndIf
		If c_Adhesive_scrap != None
			playerRef.AddItem(c_Adhesive_scrap, 5, True)
		EndIf
		If c_Copper_scrap != None
			playerRef.AddItem(c_Copper_scrap, 10, True)
		EndIf
		If Fuse != None
			playerRef.AddItem(Fuse, 1, True)
		EndIf
	EndIf
EndFunction

Function Fragment_Stage_0001_Item_00()
	FS01_GiveItem(FS01_MQ_Warn_BrokenUplinkMiscItem, 1)
EndFunction

Function Fragment_Stage_0010_Item_00()
	FS01_RunOnStart()
EndFunction

Function Fragment_Stage_0025_Item_00()
	FS01_SetMajorCheckpoint(25)
	SetObjectiveDisplayed(50, True, True)
	FS01_StartScene(FS01_MQ_Warn_AbbieIntroScene)
EndFunction

Function Fragment_Stage_0050_Item_00()
	FS01_SetMajorCheckpoint(25)
	SetObjectiveDisplayed(50, True, True)
	FS01_StartScene(FS01_MQ_Warn_AbbieIntroScene)
EndFunction

Function Fragment_Stage_0060_Item_00()
	FS01_GiveRaleighPassword()
EndFunction

Function Fragment_Stage_0100_Item_00()
	FS01_SetMajorCheckpoint(100)
	SetObjectiveCompleted(50, True)
	SetObjectiveDisplayed(100, True, True)
EndFunction

Function Fragment_Stage_0150_Item_00()
	FS01_SetMajorCheckpoint(150)
	SetObjectiveCompleted(100, True)
	SetObjectiveDisplayed(150, True, True)
	FS01_GiveItem(FS01_MQ_Warn_BrokenUplinkMiscItem, 1)
	If !GetStageDone(200)
		SetStage(200)
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	Actor playerRef = FS01_GetPlayer()
	FS01_SetMajorCheckpoint(200)
	SetObjectiveCompleted(150, True)
	SetObjectiveDisplayed(200, True, True)
	If playerRef != None
		If FS01_MQ_Warn_BrokenUplinkMiscItem != None && playerRef.GetItemCount(FS01_MQ_Warn_BrokenUplinkMiscItem) > 0
			playerRef.RemoveItem(FS01_MQ_Warn_BrokenUplinkMiscItem, 1, True)
		EndIf
		FS01_GiveItem(FS01_Warn_UplinkMiscItem, 1)
	EndIf
EndFunction

Function Fragment_Stage_0220_Item_00()
	SetObjectiveCompleted(200, True)
	SetObjectiveDisplayed(220, True, True)
	FS01_StartScene(FS01_Warn_AbbieNextStepScene)
EndFunction

Function Fragment_Stage_0250_Item_00()
	FS01_SetMajorCheckpoint(250)
	SetObjectiveCompleted(220, True)
	SetObjectiveDisplayed(250, True, True)
	FS01_GiveRaleighPassword()
	If RaleighBunkerMapMarkerREF != None
		RaleighBunkerMapMarkerREF.AddToMap(False)
	EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(250, True)
	SetObjectiveDisplayed(300, True, True)
	FS01_StartScene(FS01_Hope_AbbieRaleighsBunkerScene)
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(300, True)
	SetObjectiveDisplayed(400, True, True)
	If !GetStageDone(455)
		SetObjectiveDisplayed(450, True, True)
	EndIf
EndFunction

Function Fragment_Stage_0405_Item_00()
	SetObjectiveCompleted(400, True)
	If !GetStageDone(500)
		SetStage(500)
	EndIf
EndFunction

Function Fragment_Stage_0450_Item_00()
	If !GetStageDone(455)
		SetObjectiveDisplayed(450, True, True)
	EndIf
EndFunction

Function Fragment_Stage_0455_Item_00()
	SetObjectiveCompleted(450, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
	FS01_EnterAmplifierSearch(False)
EndFunction

Function Fragment_Stage_0500_Item_01()
	FS01_EnterAmplifierSearch(True)
EndFunction

Function Fragment_Stage_0510_Item_00()
	FS01_SetPlayerValue(FS01_CheckpointAbbieAmplifier01, 1.0)
	FS01_UpdateAbbieHeatingCoils()
	FS01_CheckAmplifiersComplete()
EndFunction

Function Fragment_Stage_0520_Item_00()
	FS01_SetPlayerValue(FS01_CheckpointAbbieAmplifier02, 1.0)
	FS01_UpdateAbbieHeatingCoils()
	FS01_CheckAmplifiersComplete()
EndFunction

Function Fragment_Stage_0530_Item_00()
	FS01_SetPlayerValue(FS01_CheckpointEllaAmplifier03, 1.0)
	SetObjectiveCompleted(501, True)
	FS01_CheckAmplifiersComplete()
EndFunction

Function Fragment_Stage_0540_Item_00()
	FS01_SetPlayerValue(FS01_CheckpointRaleighAmplifier04, 1.0)
	SetObjectiveCompleted(500, True)
	FS01_CheckAmplifiersComplete()
EndFunction

Function Fragment_Stage_0550_Item_00()
	FS01_SetPlayerValue(FS01_CheckpointRelayAmplifier05, 1.0)
	SetObjectiveCompleted(502, True)
	FS01_CheckAmplifiersComplete()
EndFunction

Function Fragment_Stage_0560_Item_00()
	SetObjectiveCompleted(500, True)
	SetObjectiveCompleted(501, True)
	SetObjectiveCompleted(502, True)
	SetObjectiveCompleted(503, True)
	If !GetStageDone(600)
		SetStage(600)
	EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
	FS01_SetMajorCheckpoint(600)
	SetObjectiveCompleted(500, True)
	SetObjectiveCompleted(501, True)
	SetObjectiveCompleted(502, True)
	SetObjectiveCompleted(503, True)
	SetObjectiveDisplayed(600, True, True)
	If !GetStageDone(601)
		SetStage(601)
	EndIf
	If !GetStageDone(700)
		SetStage(700)
	EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
	FS01_SetMajorCheckpoint(700)
	SetObjectiveCompleted(600, True)
	SetObjectiveDisplayed(700, True, True)
EndFunction

Function Fragment_Stage_0800_Item_00()
	Actor playerRef = FS01_GetPlayer()
	SetObjectiveCompleted(700, True)
	Quest reassembly = Game.GetFormFromFile(0x004E0719, "SeventySix.esm") as Quest
	If playerRef != None && FS02_MQ_Overdue_QuestStartKeyword != None
		If reassembly == None || (!reassembly.IsRunning() && !reassembly.GetStageDone(1000))
			FS02_MQ_Overdue_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
		EndIf
	EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
	FS01_SetPlayerValue(FS01_Warn_QuestCompletedValue, 1.0)
	CompleteAllObjectives()
	CompleteQuest()
	Stop()
EndFunction
