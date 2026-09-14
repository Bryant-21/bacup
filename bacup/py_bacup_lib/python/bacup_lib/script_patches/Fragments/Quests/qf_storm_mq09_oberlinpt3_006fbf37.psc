ObjectReference Function PlayerReference()
	Return Alias_Player.GetReference()
EndFunction

Function SeedEvidenceAlias(ReferenceAlias evidenceAlias, Form evidenceForm)
	If evidenceAlias.GetReference()
		Return
	EndIf

	ObjectReference corpse = Alias_DanCorpse.GetReference()
	If corpse == None
		Return
	EndIf

	ObjectReference evidenceRef = corpse.PlaceAtMe(evidenceForm, 1, False, True)
	If evidenceRef
		evidenceAlias.ForceRefTo(evidenceRef)
		corpse.AddItem(evidenceRef, 1, True)
	EndIf
EndFunction

Function AdvanceWhenEvidenceCollected()
	If IsStageDone(350) && IsStageDone(360) && !IsStageDone(600)
		SetStage(600)
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(25)
EndFunction

Function Fragment_Stage_0310_Item_00()
	SetObjectiveCompleted(25)
	SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0311_Item_00()
	SeedEvidenceAlias(Alias_Ref_Quest_QO_Note, Storm_MQ09_OberlinsReport)
	SeedEvidenceAlias(Alias_Ref_Quest_QO_Holotape, Storm_MQ09_Dan_Holotape)
EndFunction

Function Fragment_Stage_0315_Item_00()
	SetObjectiveCompleted(30)
	SetObjectiveDisplayed(31)
EndFunction

Function Fragment_Stage_0320_Item_00()
	SetObjectiveCompleted(31)
	SetObjectiveDisplayed(32)
EndFunction

Function Fragment_Stage_0330_Item_00()
	SetObjectiveCompleted(32)
	SetObjectiveDisplayed(33)
EndFunction

Function Fragment_Stage_0340_Item_00()
	SetObjectiveCompleted(33)
	SetObjectiveDisplayed(34)
	SetObjectiveDisplayed(35)
EndFunction

Function Fragment_Stage_0350_Item_00()
	SetObjectiveCompleted(34)
EndFunction

Function Fragment_Stage_0350_Item_01()
	AdvanceWhenEvidenceCollected()
EndFunction

Function Fragment_Stage_0360_Item_00()
	SetObjectiveCompleted(35)
EndFunction

Function Fragment_Stage_0360_Item_01()
	AdvanceWhenEvidenceCollected()
EndFunction

Function Fragment_Stage_0600_Item_00()
	SetObjectiveCompleted(34)
	SetObjectiveCompleted(35)
	SetObjectiveDisplayed(50)
	SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0605_Item_00()
	PlayerReference().AddItem(Caps001, 50, False)
EndFunction

Function Fragment_Stage_0610_Item_00()
	SetObjectiveCompleted(60)
EndFunction

Function Fragment_Stage_0620_Item_00()
	PlayerReference().SetValue(Storm_MQ09_OberlinChoice, 1.0)
EndFunction

Function Fragment_Stage_0630_Item_00()
	PlayerReference().SetValue(Storm_MQ09_OberlinChoice, 2.0)
EndFunction

Function Fragment_Stage_0700_Item_00()
	SetObjectiveCompleted(50)
	SetObjectiveCompleted(60)
	If !IsStageDone(9000)
		SetStage(9000)
	EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
	ObjectReference player = PlayerReference()
	Bool startFinale = False
	Float trackerValue = player.GetValue(Storm_MQ00_QuestProgressionTracker)
	If trackerValue < 2.0
		trackerValue += 1.0
		If trackerValue > 2.0
			trackerValue = 2.0
		EndIf
		player.SetValue(Storm_MQ00_QuestProgressionTracker, trackerValue)
		If trackerValue >= 2.0
			startFinale = True
		EndIf
	EndIf
	player.SetValue(Storm_MQ_HildaAwayValue, 0.0)
	If startFinale
		Storm_MQ13_Finale_StartKeyword.SendStoryEvent()
	EndIf
EndFunction

Function Fragment_Stage_9999_Item_00()
	Alias_RefCol_DisablePlacedEnemies.RemoveAll()
EndFunction
