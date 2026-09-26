Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(100, True)
	SetObjectiveDisplayed(200, True)
	Quests:MTR04:Chow chowQuest = (Self as Quest) as Quests:MTR04:Chow
	If chowQuest != None
		chowQuest.EvaluateBoothWelcome()
	ElseIf BoothWelcome != None
		BoothWelcome.Start()
	EndIf
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
	SetObjectiveDisplayed(300, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(300, True)
	SetObjectiveDisplayed(400, True)
	Quests:MTR04:Chow chowQuest = (Self as Quest) as Quests:MTR04:Chow
	If chowQuest != None
		chowQuest.BeginEating()
	EndIf
	StartChowing.Start()
EndFunction

Function Fragment_Stage_0900_Item_00()
	SetObjectiveFailed(400, True)
	Quests:MTR04:Chow chowQuest = (Self as Quest) as Quests:MTR04:Chow
	If chowQuest != None
		chowQuest.FinishEating()
		chowQuest.ScheduleShutdown()
	EndIf
	ChowFailure.Start()
EndFunction

Function Fragment_Stage_1000_Item_00()
	SetObjectiveCompleted(400, True)
	Quests:MTR04:Chow chowQuest = (Self as Quest) as Quests:MTR04:Chow
	If chowQuest != None
		chowQuest.FinishEating()
		chowQuest.ScheduleShutdown()
	EndIf
	ChowSuccess.Start()
EndFunction

Function Fragment_Stage_1500_Item_00()
	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef != None && CanEatHotdog != None
		playerRef.SetValue(CanEatHotdog, 0.0)
	EndIf
EndFunction
