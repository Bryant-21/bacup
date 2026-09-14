Function Fragment_Stage_0200_Item_00()
	If BURN_SQ01 != None && !BURN_SQ01.IsStageDone(200)
		BURN_SQ01.SetStage(200)
	EndIf
	If BURN_SQ01 == None || BURN_SQ01.IsStageDone(200)
		SetStage(9999)
	EndIf
EndFunction

Function Fragment_Stage_9999_Item_00()
	Stop()
EndFunction
