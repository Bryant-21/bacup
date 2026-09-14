Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(10, True)
	If Scene_Intro != None
		Scene_Intro.Start()
	EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
	Actor playerRef = Alias_Player.GetActorReference()
	Quests:U01A_Brewing:DailyScript dailyScript = (Self as Quest) as Quests:U01A_Brewing:DailyScript
	If playerRef != None && dailyScript != None
		If dailyScript.ChosenAlcohol.FreshDrink != None && playerRef.GetItemCount(dailyScript.ChosenAlcohol.FreshDrink) > 0
			playerRef.RemoveItem(dailyScript.ChosenAlcohol.FreshDrink, 1, True)
		ElseIf dailyScript.ChosenAlcohol.VintageDrink != None && playerRef.GetItemCount(dailyScript.ChosenAlcohol.VintageDrink) > 0
			playerRef.RemoveItem(dailyScript.ChosenAlcohol.VintageDrink, 1, True)
		EndIf
		dailyScript.HasAlcohol = False
	EndIf
	SetObjectiveCompleted(30, True)
EndFunction

Function Fragment_Stage_9000_Item_00()
	Quests:U01A_Brewing:DailyScript dailyScript = (Self as Quest) as Quests:U01A_Brewing:DailyScript
	If dailyScript != None
		dailyScript.CompleteDailyQuest(True)
	EndIf
	Stop()
EndFunction
