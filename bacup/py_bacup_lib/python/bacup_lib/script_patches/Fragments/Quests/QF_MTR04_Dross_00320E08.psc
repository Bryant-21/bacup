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
	Quests:MTR04:Dross drossQuest = (Self as Quest) as Quests:MTR04:Dross
	If drossQuest != None
		drossQuest.BeginThrowing()
	EndIf
	StartThrowing.Start()
EndFunction

Function Fragment_Stage_0310_Item_00()
	If OBJDrossTossSuccessfulThrow != None
		OBJDrossTossSuccessfulThrow.Play(Alias_LeftTire.GetReference())
	EndIf
EndFunction

Function Fragment_Stage_0320_Item_00()
	If OBJDrossTossSuccessfulThrow != None
		OBJDrossTossSuccessfulThrow.Play(Alias_RightTire.GetReference())
	EndIf
EndFunction

Function Fragment_Stage_0330_Item_00()
	If OBJDrossTossSuccessfulThrow != None
		OBJDrossTossSuccessfulThrow.Play(Alias_CenterTire.GetReference())
	EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(300, True)
	Quests:MTR04:Dross drossQuest = (Self as Quest) as Quests:MTR04:Dross
	If drossQuest != None
		drossQuest.ScoreGame()
	EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef != None
		playerRef.RemoveItem(pMTR04_DrossThrownItem, -1, True)
	EndIf
	Dross_Failure.Start()
EndFunction

Function Fragment_Stage_1000_Item_00()
	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef != None
		playerRef.RemoveItem(pMTR04_DrossThrownItem, -1, True)
	EndIf
	If OBJDrossTossWin != None
		OBJDrossTossWin.Play(playerRef)
	EndIf
	DrossSuccess.Start()
EndFunction

Function Fragment_Stage_1500_Item_00()
	Return
EndFunction
