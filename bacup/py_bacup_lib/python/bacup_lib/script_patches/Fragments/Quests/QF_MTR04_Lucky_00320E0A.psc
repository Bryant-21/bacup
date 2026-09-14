Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(100, True)
	SetObjectiveDisplayed(200, True)
	BoothWelcome.Start()
EndFunction

Function Fragment_Stage_0201_Item_00()
	SetObjectiveDisplayed(500, True)
EndFunction

Function Fragment_Stage_0202_Item_00()
	SetObjectiveDisplayed(600, True)
EndFunction

Function Fragment_Stage_0250_Item_00()
	SetObjectiveCompleted(200, True)
	SetObjectiveCompleted(500, True)
	SetObjectiveCompleted(600, True)
	SetObjectiveDisplayed(250, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(250, True)
	SetObjectiveDisplayed(300, True)
	StartRunning.Start()
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(300, True)
	SetObjectiveDisplayed(400, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveCompleted(400, True)
	LuckySuccess.Start()
EndFunction

Function Fragment_Stage_0900_Item_00()
	SetObjectiveFailed(300, True)
	Stop()
EndFunction

Function Fragment_Stage_1000_Item_00()
	CompleteQuest()
	Stop()
EndFunction

Function Fragment_Stage_1500_Item_00()
	Return
EndFunction
