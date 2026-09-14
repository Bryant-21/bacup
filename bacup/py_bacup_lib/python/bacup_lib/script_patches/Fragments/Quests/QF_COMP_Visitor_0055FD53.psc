Function Fragment_Stage_0100_Item_00()
	If !IsObjectiveDisplayed(100) && !IsObjectiveCompleted(100) && !IsObjectiveFailed(100)
		SetObjectiveDisplayed(100)
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	If !IsObjectiveCompleted(100)
		SetObjectiveCompleted(100)
	EndIf
	If !IsStageDone(9000)
		SetStage(9000)
	EndIf
EndFunction

Function Fragment_Stage_8000_Item_00()
	If !IsObjectiveCompleted(100) && !IsObjectiveFailed(100)
		SetObjectiveFailed(100)
	EndIf
	If !IsStageDone(9000)
		SetStage(9000)
	EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
	If !IsStopped()
		Stop()
	EndIf
EndFunction
