Function Fragment_Stage_0000_Item_00()
	SetStage(100)
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
	If MrClarkScene_200 != None
		MrClarkScene_200.Start()
	EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(10, True)
	SetObjectiveDisplayed(20, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(20, True)
	SetObjectiveDisplayed(30, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveCompleted(30, True)
	ObjectReference playerRef = None
	If questOwningPlayer != None
		playerRef = questOwningPlayer.GetReference()
	EndIf
	If playerRef != None && QuestCompletedValue != None
		playerRef.SetValue(QuestCompletedValue, 1.0)
	EndIf
	SetStage(9000)
EndFunction

Function Fragment_Stage_9000_Item_00()
	Stop()
EndFunction
