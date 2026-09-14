Function Fragment_Stage_0001_Item_00()
	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef != None && pBoSz01StartedAV != None
		playerRef.SetValue(pBoSz01StartedAV, 1.0)
	EndIf
	If !IsStageDone(100)
		SetStage(100)
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(50, True)
EndFunction

Function Fragment_Stage_0150_Item_00()
	SetObjectiveCompleted(50, True)
	SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(100, True)
	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef != None && BoSTechnicalSchematicQuestItem != None
		playerRef.RemoveItem(BoSTechnicalSchematicQuestItem, 1, True)
	EndIf
	If playerRef != None && pBoSTechnicalDocumentOrig != None && playerRef.GetItemCount(pBoSTechnicalDocumentOrig) >= 2
		SetStage(310)
	Else
		SetStage(300)
	EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetStage(400)
EndFunction

Function Fragment_Stage_0310_Item_00()
	SetStage(400)
EndFunction

Function Fragment_Stage_0350_Item_00()
	SetStage(400)
EndFunction

Function Fragment_Stage_0400_Item_00()
	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef != None && pBoSz01StartedAV != None
		playerRef.SetValue(pBoSz01StartedAV, 0.0)
	EndIf
	Stop()
EndFunction
