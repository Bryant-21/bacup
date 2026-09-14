Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(100, True)
	SetObjectiveDisplayed(200, True)
	SetObjectiveDisplayed(201, True)
	SetObjectiveDisplayed(202, True)
	SetObjectiveDisplayed(203, True)
	If TW002WardenFirstScene != None && !TW002WardenFirstScene.IsPlaying()
		TW002WardenFirstScene.Start()
	EndIf
EndFunction

Function Fragment_Stage_0051_Item_00()
	GiveTape(TW002SecurityTape01)
	SetObjectiveCompleted(200, True)
	CheckSecurityStationProgress()
EndFunction

Function Fragment_Stage_0052_Item_00()
	GiveTape(TW002SecurityTape02)
	SetObjectiveCompleted(201, True)
	CheckSecurityStationProgress()
EndFunction

Function Fragment_Stage_0053_Item_00()
	GiveTape(TW002SecurityTape03)
	SetObjectiveCompleted(202, True)
	CheckSecurityStationProgress()
EndFunction

Function Fragment_Stage_0054_Item_00()
	GiveTape(TW002SecurityTape04)
	SetObjectiveCompleted(203, True)
	CheckSecurityStationProgress()
EndFunction

Function GiveTape(Holotape tape)
	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef != None && tape != None && playerRef.GetItemCount(tape) == 0
		playerRef.AddItem(tape, 1, True)
	EndIf
EndFunction

Function CheckSecurityStationProgress()
	If IsStageDone(51) && IsStageDone(52) && IsStageDone(53) && IsStageDone(54) && !IsStageDone(300)
		SetStage(300)
	EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(200, True)
	SetObjectiveCompleted(201, True)
	SetObjectiveCompleted(202, True)
	SetObjectiveCompleted(203, True)
	SetObjectiveDisplayed(300, True)
EndFunction

Function Fragment_Stage_1000_Item_00()
	SetObjectiveCompleted(300, True)
	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef != None && TW002SecurityStationPassword != None && playerRef.GetItemCount(TW002SecurityStationPassword) == 0
		playerRef.AddItem(TW002SecurityStationPassword, 1, True)
	EndIf
	If TW002WardenSecondScene != None && !TW002WardenSecondScene.IsPlaying()
		TW002WardenSecondScene.Start()
	EndIf
	If TW003 != None && !TW003.IsRunning()
		TW003.Start()
	EndIf
EndFunction
