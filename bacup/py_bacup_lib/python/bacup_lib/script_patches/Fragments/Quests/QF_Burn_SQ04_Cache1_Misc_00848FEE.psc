Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(10, True)
	If !IsStageDone(9000)
		SetStage(9000)
	EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
	Stop()
EndFunction
