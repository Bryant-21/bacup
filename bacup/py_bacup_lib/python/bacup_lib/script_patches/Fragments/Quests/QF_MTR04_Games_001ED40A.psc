Function Fragment_Stage_0001_Item_00()
	If QuestStartedAt.GetLocation() == CamdenPark.GetLocation()
		SetStage(20)
	Else
		SetStage(10)
	EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
	SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0020_Item_00()
	SetObjectiveDisplayed(20, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveCompleted(10, True)
	SetObjectiveCompleted(20, True)
	SetObjectiveDisplayed(100, True)
	Attract.Start()
EndFunction

Function Fragment_Stage_0200_Item_00()
	SecurityWelcome.Start()
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(100, True)
	SetObjectiveDisplayed(300, True)
	ObjectReference lockerRef = Locker.GetReference()
	If lockerRef != None && lockerRef.GetItemCount(UniformItem) < 1
		lockerRef.AddItem(UniformItem, 1, False)
	EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
	SetObjectiveCompleted(300, True)
	SetObjectiveDisplayed(400, True)
EndFunction

Function Fragment_Stage_0750_Item_00()
	SetObjectiveCompleted(400, True)
	If QSTMTR04TimecardPunch != None
		QSTMTR04TimecardPunch.Play(Alias_Timeclock.GetReference())
	EndIf
	SetStage(800)
EndFunction

Function Fragment_Stage_0800_Item_00()
	SetObjectiveDisplayed(500, True)
	Nag.Start()

	mtr04_gamescomplete activityTracker = (Self as Quest) as mtr04_gamescomplete
	If activityTracker != None
		activityTracker.ResetActivityCompletion()
	EndIf

	If DrossQuest.IsCompleted()
		If activityTracker != None
			activityTracker.ActivityCompleted(0)
		EndIf
	ElseIf !DrossQuest.IsRunning()
		DrossQuest.Start()
	EndIf
	If LuckyQuest.IsCompleted()
		If activityTracker != None
			activityTracker.ActivityCompleted(1)
		EndIf
	ElseIf !LuckyQuest.IsRunning()
		LuckyQuest.Start()
	EndIf
	If ChowQuest.IsCompleted()
		If activityTracker != None
			activityTracker.ActivityCompleted(2)
		EndIf
	ElseIf !ChowQuest.IsRunning()
		ChowQuest.Start()
	EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
	SetObjectiveCompleted(500, True)
	SetObjectiveDisplayed(800, True)
EndFunction

Function Fragment_Stage_1000_Item_00()
	SetObjectiveCompleted(800, True)
	SetObjectiveDisplayed(900, True)
	BossIntro.Start()
EndFunction

Function Fragment_Stage_1001_Item_00()
	SetObjectiveDisplayed(1000, True)
EndFunction

Function Fragment_Stage_2000_Item_00()
	CompleteAllObjectives()
	CompleteQuest()
	Stop()
EndFunction
