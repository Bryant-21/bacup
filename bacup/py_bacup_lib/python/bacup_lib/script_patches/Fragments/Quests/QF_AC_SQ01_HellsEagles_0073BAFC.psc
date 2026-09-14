Actor Function ActorFromAlias(ReferenceAlias actorAlias)
	If actorAlias == None
		Return None
	EndIf
	Return actorAlias.GetActorReference()
EndFunction

ObjectReference Function PlayerReference()
	Return Alias_Player.GetReference()
EndFunction

Function StartSceneIfStopped(Scene sceneToStart)
	If sceneToStart != None && !sceneToStart.IsPlaying()
		sceneToStart.Start()
	EndIf
EndFunction

Function EnableAlias(ReferenceAlias targetAlias)
	If targetAlias == None
		Return
	EndIf
	ObjectReference target = targetAlias.GetReference()
	If target != None
		target.Enable()
		Actor targetActor = target as Actor
		If targetActor != None
			targetActor.EvaluatePackage()
		EndIf
	EndIf
EndFunction

Function DisableAlias(ReferenceAlias targetAlias)
	If targetAlias != None && targetAlias.GetReference() != None
		targetAlias.GetReference().Disable()
	EndIf
EndFunction

Function EnableCollection(RefCollectionAlias targets)
	If targets == None
		Return
	EndIf
	Int index = 0
	While index < targets.GetCount()
		ObjectReference target = targets.GetAt(index)
		If target != None
			target.Enable()
			Actor targetActor = target as Actor
			If targetActor != None
				targetActor.EvaluatePackage()
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction

Function DisableCollection(RefCollectionAlias targets)
	If targets == None
		Return
	EndIf
	Int index = 0
	While index < targets.GetCount()
		ObjectReference target = targets.GetAt(index)
		If target != None
			target.Disable()
		EndIf
		index += 1
	EndWhile
EndFunction

Function GiveItemIfMissing(Form itemToGive)
	ObjectReference player = PlayerReference()
	If player != None && itemToGive != None && player.GetItemCount(itemToGive) < 1
		player.AddItem(itemToGive, 1, True)
	EndIf
EndFunction

Function RemovePlayerItem(Form itemToRemove)
	ObjectReference player = PlayerReference()
	If player != None && itemToRemove != None
		Int itemCount = player.GetItemCount(itemToRemove)
		If itemCount > 0
			player.RemoveItem(itemToRemove, itemCount, True)
		EndIf
	EndIf
EndFunction

Function MakeHostile(ReferenceAlias actorAlias)
	Actor target = ActorFromAlias(actorAlias)
	Actor player = ActorFromAlias(Alias_Player)
	If target == None
		Return
	EndIf
	If BloodEagleFaction != None
		target.AddToFaction(BloodEagleFaction)
	EndIf
	target.SetGhost(False)
	target.EvaluatePackage()
	If player != None
		target.StartCombat(player)
	EndIf
EndFunction

Function ClearHostility(ReferenceAlias actorAlias)
	Actor target = ActorFromAlias(actorAlias)
	If target != None
		target.StopCombat()
		If BloodEagleFaction != None
			target.RemoveFromFaction(BloodEagleFaction)
		EndIf
		target.EvaluatePackage()
	EndIf
EndFunction

Function MoveActorToAlias(ReferenceAlias actorAlias, ReferenceAlias markerAlias)
	Actor target = ActorFromAlias(actorAlias)
	ObjectReference marker = markerAlias.GetReference()
	If target != None && marker != None
		target.Enable()
		target.MoveTo(marker)
		target.EvaluatePackage()
	EndIf
EndFunction

Function AddInterviewAnger()
	Actor player = ActorFromAlias(Alias_Player)
	If player != None && AC_SQ01_LittleRob_Anger_AV != None
		player.SetValue(AC_SQ01_LittleRob_Anger_AV, player.GetValue(AC_SQ01_LittleRob_Anger_AV) + 1.0)
	EndIf
EndFunction

Function Fragment_Stage_0001_Item_00()
	SetStage(900)
EndFunction

Function Fragment_Stage_0002_Item_00()
	SetStage(1200)
EndFunction

Function Fragment_Stage_0050_Item_00()
	SetObjectiveDisplayed(5)
	SetObjectiveDisplayed(7)
EndFunction

Function Fragment_Stage_0075_Item_00()
	SetObjectiveDisplayed(5)
EndFunction

Function Fragment_Stage_0080_Item_00()
	SetObjectiveDisplayed(7)
	EnableAlias(Alias_Actor_OscarGonzalez)
	StartSceneIfStopped(AC_SQ01_HellsEagles_Oscar_Approach_Scene)
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveCompleted(5)
	SetObjectiveCompleted(7)
	SetObjectiveDisplayed(10)
	EnableAlias(Alias_Actor_OscarGonzalez)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(10)
	GiveItemIfMissing(AC_SQ01_HellsEagles_Investigation01_Holotape)
	SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(30)
	SetObjectiveDisplayed(40)
	EnableAlias(Alias_Actor_Surly)
	StartSceneIfStopped(AC_SQ01_HellsEagles_Surly_OnEnter_Scene)
EndFunction

Function Fragment_Stage_0450_Item_00()
	SetObjectiveCompleted(40)
	GiveItemIfMissing(SurlysChem)
	SetObjectiveDisplayed(45)
EndFunction

Function Fragment_Stage_0460_Item_00()
	SetObjectiveCompleted(45)
	StartSceneIfStopped(AC_SQ01_HellsEagles_Surly_ChemFailure_Scene)
EndFunction

Function Fragment_Stage_0470_Item_00()
	SetObjectiveDisplayed(50)
	MakeHostile(Alias_Actor_Surly)
EndFunction

Function Fragment_Stage_0480_Item_00()
	SetObjectiveCompleted(40)
	SetObjectiveCompleted(45)
	SetObjectiveDisplayed(50)
	MakeHostile(Alias_Actor_Surly)
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveCompleted(40)
	SetObjectiveDisplayed(50)
	MakeHostile(Alias_Actor_Surly)
EndFunction

Function Fragment_Stage_0600_Item_00()
	SetObjectiveCompleted(50)
	SetObjectiveDisplayed(60)
	ClearHostility(Alias_Actor_Surly)
EndFunction

Function Fragment_Stage_0700_Item_00()
	SetObjectiveCompleted(60)
	SetObjectiveDisplayed(70)
	StartSceneIfStopped(AC_SQ01_HellsEagles_Surly_FoundNote_Scene)
EndFunction

Function Fragment_Stage_0750_Item_00()
	MakeHostile(Alias_Actor_Surly)
EndFunction

Function Fragment_Stage_0800_Item_00()
	SetObjectiveCompleted(70)
	SetObjectiveDisplayed(80)
EndFunction

Function Fragment_Stage_0900_Item_00()
	SetObjectiveCompleted(80)
	SetObjectiveDisplayed(90)
	EnableCollection(Alias_Actors_Guards)
EndFunction

Function Fragment_Stage_0950_Item_00()
	EnableCollection(Alias_Actors_Guards)
EndFunction

Function Fragment_Stage_1000_Item_00()
	SetObjectiveCompleted(90)
	SetObjectiveDisplayed(100)
	DisableCollection(Alias_Actors_GuardsToDisable)
	ObjectReference latrineDoor = Alias_Door_Latrine.GetReference()
	If latrineDoor != None
		latrineDoor.Lock(False)
	EndIf
	MoveActorToAlias(Alias_Actor_JackHunter, Alias_Marker_Jack_Freed)
	StartSceneIfStopped(AC_SQ01_HellsEagles_Jack_FreedTravel_Scene)
EndFunction

Function Fragment_Stage_1100_Item_00()
	SetObjectiveCompleted(100)
	SetObjectiveDisplayed(110)
	MoveActorToAlias(Alias_Actor_JackHunter, Alias_Marker_Jack_Throne)
	StartSceneIfStopped(AC_SQ01_HellsEagles_Jack_ThroneTravel_Scene)
EndFunction

Function Fragment_Stage_1200_Item_00()
	SetObjectiveCompleted(110)
	SetObjectiveDisplayed(120)
	EnableAlias(Alias_Actor_LittleRob)
	StartSceneIfStopped(AC_SQ01_HellsEagles_Rob_Approach_Scene)
EndFunction

Function Fragment_Stage_1240_Item_00()
	AddInterviewAnger()
EndFunction

Function Fragment_Stage_1260_Item_00()
	AddInterviewAnger()
EndFunction

Function Fragment_Stage_1300_Item_00()
	SetObjectiveCompleted(120)
	SetObjectiveDisplayed(130)
	MakeHostile(Alias_Actor_LittleRob)
EndFunction

Function Fragment_Stage_1400_Item_00()
	SetObjectiveCompleted(120)
	SetObjectiveCompleted(130)
	SetObjectiveDisplayed(140)
	ClearHostility(Alias_Actor_LittleRob)
	Actor player = ActorFromAlias(Alias_Player)
	If player != None && AC_SQ01_CompletedInterview_AV != None
		player.SetValue(AC_SQ01_CompletedInterview_AV, 1.0)
	EndIf
EndFunction

Function Fragment_Stage_1450_Item_00()
	GiveItemIfMissing(AC_SQ01_HellsEagles_ToOscarNote)
	StartSceneIfStopped(AC_SQ01_HellsEagles_Jack_TravelToExit_Scene)
EndFunction

Function Fragment_Stage_1500_Item_00()
	SetObjectiveCompleted(140)
	SetObjectiveDisplayed(150)
	EnableAlias(Alias_Actor_OscarGonzalez)
EndFunction

Function Fragment_Stage_1525_Item_00()
	DisableAlias(Alias_Actor_JackHunter)
EndFunction

Function Fragment_Stage_1550_Item_00()
	RemovePlayerItem(AC_SQ01_HellsEagles_ToOscarNote)
	StartSceneIfStopped(AC_SQ01_HellsEagles_Oscar_TravelToExit_Scene)
EndFunction

Function Fragment_Stage_1600_Item_00()
	DisableAlias(Alias_Actor_JackHunter)
	DisableAlias(Alias_Actor_LittleRob)
EndFunction

Function Fragment_Stage_1700_Item_00()
	Actor oscar = ActorFromAlias(Alias_Actor_OscarGonzalez)
	If oscar != None
		If AC_SQ01_HellsEagles_Oscar_AwayValue != None
			oscar.SetValue(AC_SQ01_HellsEagles_Oscar_AwayValue, 1.0)
		EndIf
		oscar.Disable()
	EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
	SetObjectiveCompleted(150)
EndFunction

Function Fragment_Stage_9001_Item_00()
	CompleteAllObjectives()
EndFunction

Function Fragment_Stage_10000_Item_00()
	RemovePlayerItem(AC_SQ01_HellsEagles_Investigation01_Holotape)
	RemovePlayerItem(SurlysChem)
	RemovePlayerItem(AC_SQ01_HellsEagles_ToOscarNote)
	DisableAlias(Alias_Actor_JackHunter)
	DisableAlias(Alias_Actor_LittleRob)
	Stop()
EndFunction
