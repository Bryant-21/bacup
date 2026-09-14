Function Fragment_Stage_0100_Item_00()
	ObjectReference playerRef = Alias_Player.GetReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If Storm_MQ01_Breadcrumb_Radio != None && !Storm_MQ01_Breadcrumb_Radio.IsRunning() && !Storm_MQ01_Breadcrumb_Radio.IsCompleted()
		Storm_MQ01_Breadcrumb_Radio_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
	EndIf
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(30)
	SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveCompleted(40)
	SetObjectiveCompleted(50)
	If Storm_MQ01_Breadcrumb_Radio != None && Storm_MQ01_Breadcrumb_Radio.IsRunning()
		Storm_MQ01_Breadcrumb_Radio.Stop()
	EndIf
	ObjectReference playerRef = Alias_Player.GetReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If Storm_MQ02_IntroPt1 != None && !Storm_MQ02_IntroPt1.IsRunning() && !Storm_MQ02_IntroPt1.IsCompleted()
		Storm_MQ02_IntroPt1_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
	EndIf
	If !IsStageDone(9000)
		SetStage(9000)
	EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
	CompleteAllObjectives()
	Stop()
EndFunction

Function Fragment_Stage_9900_Item_00()
	If Storm_MQ01_Breadcrumb_Radio != None && Storm_MQ01_Breadcrumb_Radio.IsRunning()
		Storm_MQ01_Breadcrumb_Radio.Stop()
	EndIf
EndFunction
