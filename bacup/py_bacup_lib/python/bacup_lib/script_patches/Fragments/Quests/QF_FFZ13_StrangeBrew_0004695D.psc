Function Fragment_Stage_0001_Item_00()
	Location startLocation = None
	Location teapotLocation = None

	If QuestStartedAt != None
		startLocation = QuestStartedAt.GetLocation()
	EndIf
	If Teapot != None
		teapotLocation = Teapot.GetLocation()
	EndIf

	If startLocation != None && teapotLocation != None && startLocation == teapotLocation
		SetStage(10)
	Else
		SetStage(5)
	EndIf
EndFunction

Function Fragment_Stage_0005_Item_00()
	SetObjectiveDisplayed(2, True)
EndFunction

Function Fragment_Stage_0007_Item_00()
	SetObjectiveCompleted(2, True)
	SetObjectiveDisplayed(8, True)
EndFunction

Function Fragment_Stage_0010_Item_00()
	SetObjectiveDisplayed(8, True)
EndFunction

Function Fragment_Stage_0015_Item_00()
	If WelcomeScene != None
		WelcomeScene.Start()
		While WelcomeScene.IsPlaying()
			Utility.Wait(0.5)
		EndWhile
	EndIf

	If !IsStageDone(30)
		SetStage(30)
	EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
	SetObjectiveCompleted(8, True)
	SetObjectiveDisplayed(10, True)

	Actor playerRef = None
	If Alias_QuestPlayer != None
		playerRef = Alias_QuestPlayer.GetActorReference()
	EndIf
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef != None && Honey != None && playerRef.GetItemCount(Honey) >= 10
		SetStage(50)
	EndIf
EndFunction

Function Fragment_Stage_0041_Item_00()
	ffz13_questscript questScript = (Self as Quest) as ffz13_questscript
	If questScript != None
		questScript.DisableHiveMarker(0)
	EndIf
EndFunction

Function Fragment_Stage_0042_Item_00()
	ffz13_questscript questScript = (Self as Quest) as ffz13_questscript
	If questScript != None
		questScript.DisableHiveMarker(1)
	EndIf
EndFunction

Function Fragment_Stage_0043_Item_00()
	ffz13_questscript questScript = (Self as Quest) as ffz13_questscript
	If questScript != None
		questScript.DisableHiveMarker(2)
	EndIf
EndFunction

Function Fragment_Stage_0044_Item_00()
	ffz13_questscript questScript = (Self as Quest) as ffz13_questscript
	If questScript != None
		questScript.DisableHiveMarker(3)
	EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
	SetObjectiveCompleted(10, True)
	SetObjectiveDisplayed(20, True)
EndFunction

Function Fragment_Stage_0070_Item_00()
	Actor playerRef = None
	If Alias_QuestPlayer != None
		playerRef = Alias_QuestPlayer.GetActorReference()
	EndIf
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf

	If playerRef != None && Honey != None && playerRef.GetItemCount(Honey) >= 10
		playerRef.RemoveItem(Honey, 10, True)
	EndIf

	If GoodbyeScene != None
		GoodbyeScene.Start()
		While GoodbyeScene.IsPlaying()
			Utility.Wait(0.5)
		EndWhile
	EndIf

	If !IsStageDone(90)
		SetStage(90)
	EndIf
EndFunction

Function Fragment_Stage_0090_Item_00()
	CompleteAllObjectives()

	Actor playerRef = None
	If Alias_QuestPlayer != None
		playerRef = Alias_QuestPlayer.GetActorReference()
	EndIf
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef != None && FFZ13_Brew_Completed != None
		playerRef.SetValue(FFZ13_Brew_Completed, 1.0)
	EndIf

	Stop()
EndFunction
