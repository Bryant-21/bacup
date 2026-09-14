Function Fragment_Stage_0100_Item_00()
	If Scene_Start != None
		Scene_Start.Start()
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	Quests:U01A_Brewing:DailyScript dailyScript = (Self as Quest) as Quests:U01A_Brewing:DailyScript
	If dailyScript != None
		dailyScript.BeginTipsyTest()
	EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
	Quests:U01A_Brewing:DailyScript dailyScript = (Self as Quest) as Quests:U01A_Brewing:DailyScript
	If dailyScript != None
		dailyScript.BeginTipsyTest()
	EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
	Quests:U01A_Brewing:DailyScript dailyScript = (Self as Quest) as Quests:U01A_Brewing:DailyScript
	If dailyScript != None
		dailyScript.BeginTipsyTest()
	EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
	Quests:U01A_Brewing:DailyScript dailyScript = (Self as Quest) as Quests:U01A_Brewing:DailyScript
	If dailyScript != None
		dailyScript.BeginTipsyTest()
	EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
	Quests:U01A_Brewing:DailyScript dailyScript = (Self as Quest) as Quests:U01A_Brewing:DailyScript
	If dailyScript != None
		dailyScript.BeginTipsyTest()
	EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
	Quests:U01A_Brewing:DailyScript dailyScript = (Self as Quest) as Quests:U01A_Brewing:DailyScript
	If dailyScript != None
		dailyScript.BeginTipsyTest()
	EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
	Quests:U01A_Brewing:DailyScript dailyScript = (Self as Quest) as Quests:U01A_Brewing:DailyScript
	If dailyScript != None
		dailyScript.BeginTipsyTest()
	EndIf
EndFunction

Function Fragment_Stage_8000_Item_00()
	Quests:U01A_Brewing:DailyScript dailyScript = (Self as Quest) as Quests:U01A_Brewing:DailyScript
	If dailyScript != None
		dailyScript.CompleteTipsyTest()
	EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
	SetObjectiveCompleted(100, True)
	Quests:U01A_Brewing:DailyScript dailyScript = (Self as Quest) as Quests:U01A_Brewing:DailyScript
	If dailyScript != None
		dailyScript.CompleteDailyQuest()
	EndIf
	Stop()
EndFunction
