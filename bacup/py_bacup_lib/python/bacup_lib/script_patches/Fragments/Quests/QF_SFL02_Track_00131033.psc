Function ShowObjective(Int aiObjective)
	SetObjectiveDisplayed(aiObjective, True, True)
EndFunction

Function CompleteObjective(Int aiObjective)
	SetObjectiveCompleted(aiObjective, True)
EndFunction

Function Fragment_Stage_0000_Item_00()
	If !IsStageDone(10)
		SetStage(10)
	EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		If Alias_SFL02Player.GetReference() == None
			Alias_SFL02Player.ForceRefTo(playerRef)
		EndIf
		If SFL02_Track_StartedValue != None
			playerRef.SetValue(SFL02_Track_StartedValue, 1.0)
		EndIf
	EndIf

	SetStage(25)
EndFunction

Function Fragment_Stage_0025_Item_00()
	ShowObjective(25)
EndFunction

Function Fragment_Stage_0050_Item_00()
	ShowObjective(50)
EndFunction

Function Fragment_Stage_0100_Item_00()
	CompleteObjective(25)
	CompleteObjective(50)
	ShowObjective(100)
EndFunction

Function Fragment_Stage_0150_Item_00()
	CompleteObjective(100)
	ShowObjective(200)
	If SFL02_Track_HardballGreet != None
		SFL02_Track_HardballGreet.Start()
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	ShowObjective(200)
EndFunction

Function Fragment_Stage_0200_Item_01()
	ShowObjective(200)
EndFunction

Function Fragment_Stage_0250_Item_00()
	If SFL02_Track_HardballScene != None
		SFL02_Track_HardballScene.Start()
	EndIf
EndFunction

Function Fragment_Stage_0275_Item_00()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None && SFL02_Track_VertibotTerminalCode != None && playerRef.GetItemCount(SFL02_Track_VertibotTerminalCode) == 0
		playerRef.AddItem(SFL02_Track_VertibotTerminalCode, 1, False)
	EndIf
	CompleteObjective(200)
	ShowObjective(300)
EndFunction

Function Fragment_Stage_0300_Item_00()
	ShowObjective(300)
EndFunction

Function Fragment_Stage_0350_Item_00()
	If SFL02_Track_HardballFlavorScene != None
		SFL02_Track_HardballFlavorScene.Start()
	EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
	CompleteObjective(300)
	ShowObjective(400)
EndFunction

Function Fragment_Stage_0450_Item_00()
	CompleteObjective(400)
	ShowObjective(450)
	If SFL02_Track_Vertibot_QuestStartKeyword != None && SFL02_Track_VertibotQuest != None && !SFL02_Track_VertibotQuest.IsRunning()
		SFL02_Track_Vertibot_QuestStartKeyword.SendStoryEvent()
	EndIf
EndFunction

Function Fragment_Stage_0460_Item_00()
	CompleteObjective(450)
EndFunction

Function Fragment_Stage_0500_Item_00()
	ShowObjective(500)
EndFunction

Function Fragment_Stage_0550_Item_00()
	CompleteObjective(500)
	ShowObjective(600)
EndFunction

Function Fragment_Stage_0600_Item_00()
	ShowObjective(600)
EndFunction

Function Fragment_Stage_0800_Item_00()
	CompleteObjective(700)
	ShowObjective(800)
	If SFL02_Track_Radio_QuestStartKeyword != None && SFL02_Track_RadioQuest != None && !SFL02_Track_RadioQuest.IsRunning()
		SFL02_Track_Radio_QuestStartKeyword.SendStoryEvent()
	EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
	CompleteObjective(800)
	ShowObjective(900)
	If SFL02_Track_RadioQuest != None && SFL02_Track_RadioQuest.IsRunning()
		SFL02_Track_RadioQuest.SetStage(1000)
	EndIf
EndFunction

Function Fragment_Stage_0910_Item_00()
	ShowObjective(1000)
EndFunction

Function Fragment_Stage_0920_Item_00()
	ShowObjective(1000)
EndFunction

Function Fragment_Stage_0930_Item_00()
	ShowObjective(1020)
EndFunction

Function Fragment_Stage_1000_Item_00()
	CompleteObjective(900)
	ShowObjective(1000)
EndFunction

Function Fragment_Stage_1010_Item_00()
	CompleteObjective(1000)
	ShowObjective(1010)
	If !IsStageDone(1100)
		SetStage(1100)
	EndIf
EndFunction

Function Fragment_Stage_1020_Item_00()
	ShowObjective(1050)
	If !IsStageDone(1100)
		SetStage(1100)
	EndIf
EndFunction

Function Fragment_Stage_1030_Item_00()
	ShowObjective(1010)
EndFunction

Function Fragment_Stage_1100_Item_00()
	CompleteObjective(1010)
	CompleteObjective(1020)
	CompleteObjective(1050)
	ShowObjective(1100)
EndFunction

Function Fragment_Stage_1200_Item_00()
	CompleteObjective(1100)
	ShowObjective(1200)
EndFunction

Function Fragment_Stage_1300_Item_00()
	CompleteObjective(1200)
	ShowObjective(1300)
EndFunction

Function Fragment_Stage_2000_Item_00()
	CompleteObjective(1300)
EndFunction
