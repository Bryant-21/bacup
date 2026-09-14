ObjectReference Function PlayerReference()
	Return Alias_Player.GetReference()
EndFunction

Function GiveIfMissing(Form itemToGive)
	ObjectReference player = PlayerReference()
	If itemToGive && player.GetItemCount(itemToGive) == 0
		player.AddItem(itemToGive, 1, True)
	EndIf
EndFunction

Function AdvanceWhenAllMushroomsFound()
	If IsStageDone(1410) && IsStageDone(1420) && IsStageDone(1430) && !IsStageDone(1500)
		SetStage(1500)
	EndIf
EndFunction

Function ReleaseSedatedLost(ReferenceAlias lostAlias)
	Actor lostActor = lostAlias.GetActorReference()
	If lostActor
		lostActor.RemoveFromFaction(PlayerAllyFaction)
		lostActor.Enable()
		lostActor.EvaluatePackage()
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(10)
	PlayerReference().SetValue(Storm_MQ_HildaAwayValue, 1.0)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(20)
	GiveIfMissing(Storm_MQ08_OrganicsSecurityKey)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(25)
	PlayerReference().SetValue(Storm_MQ08_HallucGasActive, 1.0)
EndFunction

Function Fragment_Stage_0350_Item_00()
	SetObjectiveCompleted(25)
	SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(30)
	SetObjectiveDisplayed(31)
	PlayerReference().SetValue(Storm_MQ08_HallucGasActive, 0.0)
EndFunction

Function Fragment_Stage_0410_Item_00()
	SetObjectiveCompleted(31)
	SetObjectiveDisplayed(32)
EndFunction

Function Fragment_Stage_0420_Item_00()
	SetObjectiveCompleted(32)
	SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveCompleted(40)
	SetObjectiveDisplayed(41)
	GiveIfMissing(Storm_MQ08_Key_OrganicsChemWing)
EndFunction

Function Fragment_Stage_0510_Item_00()
	SetObjectiveCompleted(41)
	SetObjectiveDisplayed(42)
EndFunction

Function Fragment_Stage_0520_Item_00()
	SetObjectiveCompleted(42)
	SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0600_Item_00()
	SetObjectiveCompleted(50)
	SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0700_Item_00()
	SetObjectiveCompleted(60)
	SetObjectiveDisplayed(70)
	Storm_MQ08_OberlinPt2_HildaIntercom01_Greenhouse.Start()
EndFunction

Function Fragment_Stage_0800_Item_00()
	SetObjectiveCompleted(70)
	SetObjectiveDisplayed(75)
	GiveIfMissing(Storm_MQ08_Key_OrganicsChemLabs)
EndFunction

Function Fragment_Stage_0850_Item_00()
	SetObjectiveCompleted(75)
	SetObjectiveDisplayed(80)
EndFunction

Function Fragment_Stage_0900_Item_00()
	SetObjectiveCompleted(80)
	SetObjectiveDisplayed(90)
EndFunction

Function Fragment_Stage_1000_Item_00()
	SetObjectiveCompleted(90)
	SetObjectiveDisplayed(100)
	PlayerReference().RemoveItem(Storm_MQ08_Quest_GreenhousePlant, 1, True)
	GiveIfMissing(Storm_MQ08_Quest_HildaSerum01)
EndFunction

Function Fragment_Stage_1100_Item_00()
	SetObjectiveCompleted(100)
	SetObjectiveDisplayed(110)
	PlayerReference().RemoveItem(Storm_MQ08_Quest_HildaSerum01, 1, True)
	ReleaseSedatedLost(Alias_Actor_SedatedLost01)
EndFunction

Function Fragment_Stage_1200_Item_00()
	SetObjectiveCompleted(110)
	SetObjectiveDisplayed(120)
	Storm_MQ08_OberlinPt2_HildaIntercom02_DrugsLab.Start()
EndFunction

Function Fragment_Stage_1300_Item_00()
	SetObjectiveCompleted(120)
	SetObjectiveDisplayed(121)
EndFunction

Function Fragment_Stage_1310_Item_00()
	SetObjectiveCompleted(121)
	SetObjectiveDisplayed(122)
EndFunction

Function Fragment_Stage_1320_Item_00()
	SetObjectiveCompleted(122)
	SetObjectiveDisplayed(130)
EndFunction

Function Fragment_Stage_1400_Item_00()
	SetObjectiveCompleted(130)
	SetObjectiveDisplayed(140)
	PlayerReference().SetValue(Storm_MQ08_HallucGasActive, 1.0)
EndFunction

Function Fragment_Stage_1410_Item_00()
	AdvanceWhenAllMushroomsFound()
EndFunction

Function Fragment_Stage_1410_Item_01()
	AdvanceWhenAllMushroomsFound()
EndFunction

Function Fragment_Stage_1420_Item_00()
	AdvanceWhenAllMushroomsFound()
EndFunction

Function Fragment_Stage_1420_Item_01()
	AdvanceWhenAllMushroomsFound()
EndFunction

Function Fragment_Stage_1430_Item_00()
	AdvanceWhenAllMushroomsFound()
EndFunction

Function Fragment_Stage_1430_Item_01()
	AdvanceWhenAllMushroomsFound()
EndFunction

Function Fragment_Stage_1500_Item_00()
	SetObjectiveCompleted(140)
	SetObjectiveDisplayed(150)
EndFunction

Function Fragment_Stage_1600_Item_00()
	SetObjectiveCompleted(150)
	SetObjectiveDisplayed(160)
	PlayerReference().SetValue(Storm_MQ08_HallucGasActive, 0.0)
EndFunction

Function Fragment_Stage_1700_Item_00()
	SetObjectiveCompleted(160)
	SetObjectiveDisplayed(161)
	ObjectReference player = PlayerReference()
	player.RemoveItem(Storm_MQ08_Quest_Shroom01, 1, True)
	player.RemoveItem(Storm_MQ08_Quest_Shroom02, 1, True)
	player.RemoveItem(Storm_MQ08_Quest_Shroom03, 1, True)
	GiveIfMissing(Storm_MQ08_Quest_HildaSerum02)
	GiveIfMissing(Storm_MQ08_Quest_HildaSerum03)
EndFunction

Function Fragment_Stage_1710_Item_00()
	SetObjectiveCompleted(161)
	SetObjectiveDisplayed(162)
EndFunction

Function Fragment_Stage_1715_Item_00()
	SetObjectiveCompleted(162)
	SetObjectiveDisplayed(163)
EndFunction

Function Fragment_Stage_1720_Item_00()
	SetObjectiveCompleted(163)
	SetObjectiveDisplayed(170)
EndFunction

Function Fragment_Stage_1800_Item_00()
	SetObjectiveCompleted(170)
	SetObjectiveDisplayed(180)
EndFunction

Function Fragment_Stage_1900_Item_00()
	SetObjectiveCompleted(180)
	SetObjectiveDisplayed(190)
	PlayerReference().RemoveItem(Storm_MQ08_Quest_HildaSerum02, 1, True)
	ReleaseSedatedLost(Alias_Actor_SedatedLost02)
EndFunction

Function Fragment_Stage_2000_Item_00()
	SetObjectiveCompleted(190)
	SetObjectiveDisplayed(200)
	Storm_MQ08_OberlinPt2_HildaIntercom03_Surgery.Start()
EndFunction

Function Fragment_Stage_2100_Item_00()
	SetObjectiveCompleted(200)
	SetObjectiveDisplayed(201)
	GiveIfMissing(Storm_MQ08_Key_OrganicsFluid)
EndFunction

Function Fragment_Stage_2101_Item_00()
	SetObjectiveCompleted(201)
	SetObjectiveDisplayed(205)
EndFunction

Function Fragment_Stage_2105_Item_00()
	SetObjectiveCompleted(205)
	SetObjectiveDisplayed(206)
EndFunction

Function Fragment_Stage_2106_Item_00()
	SetObjectiveCompleted(206)
	SetObjectiveDisplayed(210)
	GiveIfMissing(Storm_MQ08_Key_OrganicsVat)
EndFunction

Function Fragment_Stage_2200_Item_00()
	SetObjectiveCompleted(210)
	SetObjectiveDisplayed(220)
EndFunction

Function Fragment_Stage_2300_Item_00()
	SetObjectiveCompleted(220)
	SetObjectiveDisplayed(230)
	PlayerReference().RemoveItem(Storm_MQ08_Quest_HildaSerum03, 1, True)
	( Alias_Ref_Dummy_VatStorageCutscene as Quests:Storm:MQ08:VatStorageCutsceneDummyAliasScript ).StartCutscene()
EndFunction

Function Fragment_Stage_2400_Item_00()
	SetObjectiveCompleted(230)
	SetObjectiveDisplayed(231)
	SetObjectiveDisplayed(235)
	Storm_MQ08_OberlinPt2_HildaIntercom04_VatStorage.Start()
EndFunction

Function Fragment_Stage_2401_Item_00()
	SetObjectiveCompleted(231)
	SetObjectiveDisplayed(232)
EndFunction

Function Fragment_Stage_2402_Item_00()
	SetObjectiveCompleted(232)
EndFunction

Function Fragment_Stage_2405_Item_00()
	SetObjectiveCompleted(235)
	SetObjectiveDisplayed(240)
EndFunction

Function Fragment_Stage_2410_Item_00()
	PlayerReference().SetValue(Storm_MQ08_HildaChoice, 1.0)
EndFunction

Function Fragment_Stage_2420_Item_00()
	PlayerReference().SetValue(Storm_MQ08_HildaChoice, 2.0)
EndFunction

Function Fragment_Stage_2500_Item_00()
	SetObjectiveCompleted(240)
	SetObjectiveDisplayed(245)
	SetObjectiveDisplayed(250)
	PlayerReference().SetValue(Storm_MQ_HildaAwayValue, 0.0)
EndFunction

Function Fragment_Stage_9000_Item_00()
	SetObjectiveCompleted(245)
	SetObjectiveCompleted(250)
	Storm_MQ09_OberlinPt3_StartKeyword.SendStoryEvent()
EndFunction

Function Fragment_Stage_9999_Item_00()
	PlayerReference().SetValue(Storm_MQ08_HallucGasActive, 0.0)
	PlayerReference().SetValue(Storm_MQ_HildaAwayValue, 0.0)
EndFunction
