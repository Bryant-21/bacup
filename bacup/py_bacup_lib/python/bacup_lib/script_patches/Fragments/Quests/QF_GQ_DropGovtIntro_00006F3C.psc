Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(100, True)
	SetObjectiveDisplayed(200, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(200, True)
	If GQ_DropGovtIntroFlag != None
		Actor playerRef = Alias_Player.GetActorReference()
		If playerRef != None
			playerRef.SetValue(GQ_DropGovtIntroFlag, 1.0)
		EndIf
	EndIf
EndFunction
