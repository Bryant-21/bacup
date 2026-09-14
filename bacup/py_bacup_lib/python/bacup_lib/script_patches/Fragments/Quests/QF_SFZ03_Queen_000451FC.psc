Function Fragment_Stage_0000_Item_00()
	SFZ03_Queen_QuestScript hunt = (Self as Quest) as SFZ03_Queen_QuestScript
	If hunt != None
		hunt.ResetHuntState()
	EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
	SFZ03_Queen_QuestScript hunt = (Self as Quest) as SFZ03_Queen_QuestScript
	If hunt != None
		hunt.ResetHuntState()
		hunt.RegisterRuntimeEvents()
	EndIf

	Actor playerRef = Alias_SFZ03Player.GetActorRef()
	If playerRef != None && playerRef.GetValue(SFZ03_Queen_QuestCompletedValue) > 0.0
		SetStage(100)
	Else
		SetStage(50)
	EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
	SetObjectiveDisplayed(50, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveCompleted(50, True)
	SetObjectiveDisplayed(100, True)
	SetObjectiveDisplayed(110, True)
	SetObjectiveDisplayed(120, True)

	ObjectReference mapMarker = None
	If Alias_MapMarker != None
		mapMarker = Alias_MapMarker.GetReference()
	EndIf
	If mapMarker != None
		mapMarker.AddToMap()
	EndIf
EndFunction

Function Fragment_Stage_0110_Item_00()
	SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0120_Item_00()
	SetObjectiveDisplayed(110, True)
EndFunction

Function Fragment_Stage_0130_Item_00()
	SetObjectiveDisplayed(120, True)
EndFunction

Function Fragment_Stage_0150_Item_00()
	SFZ03_Queen_QuestScript hunt = (Self as Quest) as SFZ03_Queen_QuestScript
	If hunt != None
		hunt.InspectCryptidCollection(Alias_BossCollection01, 1)
	EndIf
EndFunction

Function Fragment_Stage_0151_Item_00()
	SFZ03_Queen_QuestScript hunt = (Self as Quest) as SFZ03_Queen_QuestScript
	If hunt != None
		hunt.InspectCryptidCollection(Alias_BossCollection01, 1)
	EndIf
EndFunction

Function Fragment_Stage_0155_Item_00()
	SFZ03_Queen_QuestScript hunt = (Self as Quest) as SFZ03_Queen_QuestScript
	If hunt != None
		hunt.InspectCryptidCollection(Alias_BossCollection01, 1)
	EndIf
	SetObjectiveCompleted(100, True)
EndFunction

Function Fragment_Stage_0160_Item_00()
	SFZ03_Queen_QuestScript hunt = (Self as Quest) as SFZ03_Queen_QuestScript
	If hunt != None
		hunt.InspectCryptidCollection(Alias_BossCollection02, 2)
	EndIf
EndFunction

Function Fragment_Stage_0161_Item_00()
	SFZ03_Queen_QuestScript hunt = (Self as Quest) as SFZ03_Queen_QuestScript
	If hunt != None
		hunt.InspectCryptidCollection(Alias_BossCollection02, 2)
	EndIf
EndFunction

Function Fragment_Stage_0165_Item_00()
	SFZ03_Queen_QuestScript hunt = (Self as Quest) as SFZ03_Queen_QuestScript
	If hunt != None
		hunt.InspectCryptidCollection(Alias_BossCollection02, 2)
	EndIf
	SetObjectiveCompleted(110, True)
EndFunction

Function Fragment_Stage_0170_Item_00()
	SFZ03_Queen_QuestScript hunt = (Self as Quest) as SFZ03_Queen_QuestScript
	If hunt != None
		hunt.InspectCryptidCollection(Alias_BossCollection03, 3)
	EndIf
EndFunction

Function Fragment_Stage_0171_Item_00()
	SFZ03_Queen_QuestScript hunt = (Self as Quest) as SFZ03_Queen_QuestScript
	If hunt != None
		hunt.InspectCryptidCollection(Alias_BossCollection03, 3)
	EndIf
EndFunction

Function Fragment_Stage_0175_Item_00()
	SFZ03_Queen_QuestScript hunt = (Self as Quest) as SFZ03_Queen_QuestScript
	If hunt != None
		hunt.InspectCryptidCollection(Alias_BossCollection03, 3)
	EndIf
	SetObjectiveCompleted(120, True)
EndFunction

Function Fragment_Stage_0180_Item_00()
	SetObjectiveCompleted(100, True)
	SetObjectiveCompleted(110, True)
	SetObjectiveCompleted(120, True)
	SetObjectiveDisplayed(180, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(180, True)
	SetObjectiveDisplayed(200, True)
EndFunction

Function Fragment_Stage_0230_Item_00()
	SFZ03_Queen_QuestScript hunt = (Self as Quest) as SFZ03_Queen_QuestScript
	If hunt != None
		hunt.SetCreatureStage(230)
	EndIf
EndFunction

Function Fragment_Stage_0240_Item_00()
	SFZ03_Queen_QuestScript hunt = (Self as Quest) as SFZ03_Queen_QuestScript
	If hunt != None
		hunt.SetCreatureStage(240)
	EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
	SFZ03_Queen_QuestScript hunt = (Self as Quest) as SFZ03_Queen_QuestScript
	If hunt != None
		hunt.SetCreatureStage(250)
	EndIf
EndFunction

Function Fragment_Stage_0260_Item_00()
	SFZ03_Queen_QuestScript hunt = (Self as Quest) as SFZ03_Queen_QuestScript
	If hunt != None
		hunt.SetCreatureStage(260)
	EndIf
EndFunction

Function Fragment_Stage_0270_Item_00()
	SFZ03_Queen_QuestScript hunt = (Self as Quest) as SFZ03_Queen_QuestScript
	If hunt != None
		hunt.SetCreatureStage(270)
	EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(200, True)
	SetObjectiveDisplayed(300, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(300, True)
	SetObjectiveDisplayed(400, True)
	If SFZ03_Queen_AnalyzerScene != None
		SFZ03_Queen_AnalyzerScene.Start()
	EndIf
EndFunction

Function Fragment_Stage_0410_Item_00()
	ObjectReference analyzerSFX = None
	If Alias_AnalyzerSFX != None
		analyzerSFX = Alias_AnalyzerSFX.GetReference()
	EndIf
	If analyzerSFX != None
		analyzerSFX.Enable(False)
	Else
		ObjectReference analyzerRef = Alias_Analyzer.GetReference()
		If analyzerRef != None
			OBJMixerMachineSpinLPM.Play(analyzerRef)
		EndIf
	EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
	SFZ03_Queen_QuestScript hunt = (Self as Quest) as SFZ03_Queen_QuestScript
	If hunt != None
		hunt.CastCryptidKnowledge()
	EndIf

	ObjectReference analyzerRef = Alias_Analyzer.GetReference()
	If analyzerRef != None
		OBJMixerMachineSpinOneshot.Play(analyzerRef)
	EndIf
	SetStage(1000)
EndFunction

Function Fragment_Stage_1000_Item_00()
	SetObjectiveCompleted(400, True)
	Actor playerRef = Alias_SFZ03Player.GetActorRef()
	If playerRef != None
		playerRef.SetValue(SFZ03_Queen_QuestCompletedValue, 1.0)
	EndIf
	Stop()
EndFunction
