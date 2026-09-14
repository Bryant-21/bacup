ObjectReference Function PlayerReference()
	ObjectReference playerRef = Alias_Player.GetReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	Return playerRef
EndFunction

Function SetPlayerValue(ActorValue valueToSet, Float value)
	Actor player = PlayerReference() as Actor
	If player != None && valueToSet != None
		player.SetValue(valueToSet, value)
	EndIf
EndFunction

Function RegisterForPlayerLocation()
	Actor player = PlayerReference() as Actor
	If player != None
		RegisterForRemoteEvent(player, "OnLocationChange")
		RegisterForRemoteEvent(player, "OnPlayerLoadGame")
	EndIf
EndFunction

Function AdvanceForPlayerLocation(Location currentLocation)
	If currentLocation == None
		Return
	EndIf
	LocationAlias highwayTown = GetAlias(10) as LocationAlias
	LocationAlias eugeneInterior = GetAlias(6) as LocationAlias
	If highwayTown != None && currentLocation == highwayTown.GetLocation()
		If IsStageDone(100) && !IsStageDone(125)
			SetStage(125)
		EndIf
	EndIf
	If eugeneInterior != None && currentLocation == eugeneInterior.GetLocation()
		If IsStageDone(600) && !IsStageDone(700)
			SetStage(700)
		ElseIf IsStageDone(150) && !IsStageDone(200)
			SetStage(200)
		EndIf
	EndIf
EndFunction

Event OnQuestInit()
	RegisterForPlayerLocation()
	Actor player = PlayerReference() as Actor
	If player != None
		AdvanceForPlayerLocation(player.GetCurrentLocation())
	EndIf
EndEvent

Event Actor.OnLocationChange(Actor akSender, Location akOldLoc, Location akNewLoc)
	If akSender == PlayerReference()
		AdvanceForPlayerLocation(akNewLoc)
	EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender == PlayerReference()
		RegisterForPlayerLocation()
		AdvanceForPlayerLocation(akSender.GetCurrentLocation())
	EndIf
EndEvent

Event OnQuestShutdown()
	Actor player = PlayerReference() as Actor
	If player != None
		UnregisterForRemoteEvent(player, "OnLocationChange")
		UnregisterForRemoteEvent(player, "OnPlayerLoadGame")
	EndIf
EndEvent

Function RestoreHighwayTownActors()
	Actor runt = Actor_Runt.GetActorReference()
	Actor runtCorpse = Actor_Runt_Corpse.GetActorReference()
	Actor eugene = Actor_Eugene.GetActorReference()
	If IsStageDone(700)
		SetPlayerValue(BURN_MQ03_RuntHWT_AV, 0.0)
		SetPlayerValue(BURN_MQ03_RuntCorpseHWT_AV, 1.0)
		SetPlayerValue(BURN_SQ02_EugeneHWT_AV, 1.0)
		If runt != None
			runt.Disable()
		EndIf
		If runtCorpse != None
			runtCorpse.Enable()
		EndIf
		If eugene != None
			eugene.Enable()
			eugene.EvaluatePackage()
		EndIf
	Else
		SetPlayerValue(BURN_MQ03_RuntHWT_AV, 1.0)
		If runt != None
			runt.Enable()
			runt.EvaluatePackage()
		EndIf
		If runtCorpse != None
			runtCorpse.Disable()
		EndIf
	EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
	RestoreHighwayTownActors()
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetPlayerValue(BURN_MQ03_RuntHWT_AV, 1.0)
	RestoreHighwayTownActors()
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0125_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(12)
EndFunction

Function Fragment_Stage_0150_Item_00()
	SetObjectiveCompleted(12)
	SetObjectiveDisplayed(15)
EndFunction

Function Fragment_Stage_0200_Item_00()
	RestoreHighwayTownActors()
	SetObjectiveCompleted(15)
	SetObjectiveDisplayed(20)
EndFunction

Burn_MQ03_MidQuestChallengeGrants Function ChallengeController()
	Return (Self as Quest) as Burn_MQ03_MidQuestChallengeGrants
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(30)
	Burn_MQ03_MidQuestChallengeGrants controller = ChallengeController()
	If controller != None
		controller.EvaluateChallengeProgress()
	EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(30)
	SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveCompleted(40)
	SetObjectiveDisplayed(50)
	Burn_MQ03_MidQuestChallengeGrants controller = ChallengeController()
	If controller != None
		controller.EvaluateChallengeProgress()
	EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
	SetObjectiveCompleted(50)
	SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0700_Item_00()
	SetObjectiveCompleted(60)
	RestoreHighwayTownActors()
	SetObjectiveDisplayed(70)
EndFunction

Function Fragment_Stage_9000_Item_00()
	CompleteAllObjectives()
	ObjectReference playerRef = PlayerReference()
	If BURN_SQ02_Outro_QuestStartKeyword != None && playerRef != None
		BURN_SQ02_Outro_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
	EndIf
	If !IsStageDone(9999)
		SetStage(9999)
	EndIf
EndFunction

Function Fragment_Stage_9999_Item_00()
	Stop()
EndFunction
