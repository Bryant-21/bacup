Burn_SQ04_Collectables_Script Function CollectableTracker()
	Return (Self as Quest) as Burn_SQ04_Collectables_Script
EndFunction

Actor Function PlayerReference()
	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	Return playerRef
EndFunction

Function ReconcileCollectables()
	Burn_SQ04_Collectables_Script tracker = CollectableTracker()
	If tracker != None
		tracker.ReconcileCollectableProgress()
	EndIf
EndFunction

Function ShowCollectObjective()
	SetObjectiveDisplayed(30, False)
	SetObjectiveDisplayed(20, True)
	SetObjectiveDisplayed(50, True)
EndFunction

Function ShowReturnObjective()
	SetObjectiveDisplayed(20, False)
	SetObjectiveDisplayed(50, False)
	SetObjectiveDisplayed(30, True)
EndFunction

Function StartCacheQuest(Keyword akStartKeyword)
	Actor playerRef = PlayerReference()
	If akStartKeyword != None && playerRef != None
		akStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(5, True)
EndFunction

Function Fragment_Stage_0101_Item_00()
	ReconcileCollectables()
EndFunction

Function Fragment_Stage_0102_Item_00()
	ReconcileCollectables()
EndFunction

Function Fragment_Stage_0104_Item_00()
	ReconcileCollectables()
EndFunction

Function Fragment_Stage_0105_Item_00()
	ReconcileCollectables()
EndFunction

Function Fragment_Stage_0106_Item_00()
	ReconcileCollectables()
EndFunction

Function Fragment_Stage_0107_Item_00()
	ReconcileCollectables()
EndFunction

Function Fragment_Stage_0110_Item_00()
	SetObjectiveDisplayed(5, True)
EndFunction

Function Fragment_Stage_0150_Item_00()
	SetObjectiveCompleted(5, True)
	SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(10, True)
	ShowCollectObjective()
	ReconcileCollectables()
EndFunction

Function Fragment_Stage_0300_Item_00()
	ShowReturnObjective()
EndFunction

Function Fragment_Stage_0400_Item_00()
	ShowCollectObjective()
EndFunction

Function Fragment_Stage_0500_Item_00()
	ShowReturnObjective()
EndFunction

Function Fragment_Stage_0600_Item_00()
	ShowCollectObjective()
EndFunction

Function Fragment_Stage_0700_Item_00()
	ShowReturnObjective()
EndFunction

Function Fragment_Stage_0800_Item_00()
	ShowCollectObjective()
EndFunction

Function Fragment_Stage_0900_Item_00()
	ShowReturnObjective()
EndFunction

Function Fragment_Stage_1000_Item_00()
	ShowCollectObjective()
EndFunction

Function Fragment_Stage_1100_Item_00()
	ShowReturnObjective()
EndFunction

Function Fragment_Stage_1200_Item_00()
	ShowCollectObjective()
EndFunction

Function Fragment_Stage_1300_Item_00()
	ShowReturnObjective()
EndFunction

Function Fragment_Stage_1400_Item_00()
	ShowCollectObjective()
EndFunction

Function Fragment_Stage_1500_Item_00()
	ShowReturnObjective()
EndFunction

Function Fragment_Stage_1600_Item_00()
	ShowCollectObjective()
	If !IsStageDone(1650)
		SetStage(1650)
	EndIf
EndFunction

Function Fragment_Stage_1650_Item_00()
	StartCacheQuest(Burn_SQ04_Cache1StartKeyword)
EndFunction

Function Fragment_Stage_1700_Item_00()
	ShowReturnObjective()
EndFunction

Function Fragment_Stage_1800_Item_00()
	ShowCollectObjective()
EndFunction

Function Fragment_Stage_1900_Item_00()
	ShowReturnObjective()
EndFunction

Function Fragment_Stage_2000_Item_00()
	ShowCollectObjective()
EndFunction

Function Fragment_Stage_2100_Item_00()
	ShowReturnObjective()
EndFunction

Function Fragment_Stage_2200_Item_00()
	ShowCollectObjective()
EndFunction

Function Fragment_Stage_2300_Item_00()
	ShowReturnObjective()
EndFunction

Function Fragment_Stage_2400_Item_00()
	ShowCollectObjective()
	If !IsStageDone(2450)
		SetStage(2450)
	EndIf
EndFunction

Function Fragment_Stage_2450_Item_00()
	StartCacheQuest(Burn_SQ04_Cache2StartKeyword)
EndFunction

Function Fragment_Stage_2500_Item_00()
	ShowReturnObjective()
EndFunction

Function Fragment_Stage_2600_Item_00()
	ShowCollectObjective()
EndFunction

Function Fragment_Stage_2700_Item_00()
	ShowReturnObjective()
EndFunction

Function Fragment_Stage_2800_Item_00()
	ShowCollectObjective()
EndFunction

Function Fragment_Stage_2900_Item_00()
	ShowReturnObjective()
EndFunction

Function Fragment_Stage_3000_Item_00()
	ShowCollectObjective()
	If !IsStageDone(3050)
		SetStage(3050)
	EndIf
EndFunction

Function Fragment_Stage_3050_Item_00()
	StartCacheQuest(Burn_SQ04_Cache3StartKeyword)
EndFunction

Function Fragment_Stage_7000_Item_00()
	ShowReturnObjective()
EndFunction

Function Fragment_Stage_7100_Item_00()
	SetObjectiveCompleted(20, True)
	SetObjectiveCompleted(30, True)
	SetObjectiveDisplayed(50, False)
	SetObjectiveDisplayed(60, True)
	WatchForExecHideout()
EndFunction

; 7200 ("Investigate the apartment" done) gates the Exec's greeting and the kill alias,
; and nothing in the record sets it; arriving in Loc_ExecHideout (alias 39) is the signal.
Location Function ExecHideoutLocation()
	LocationAlias hideoutAlias = GetAlias(39) as LocationAlias
	If hideoutAlias != None && hideoutAlias.GetLocation() != None
		Return hideoutAlias.GetLocation()
	EndIf
	Return Game.GetFormFromFile(0x008258EA, "SeventySix.esm") as Location
EndFunction

Function WatchForExecHideout()
	Actor playerRef = Game.GetPlayer()
	If playerRef == None || IsStageDone(7200)
		Return
	EndIf
	Location hideout = ExecHideoutLocation()
	If hideout != None && playerRef.IsInLocation(hideout)
		SetStage(7200)
		Return
	EndIf
	RegisterForRemoteEvent(playerRef, "OnLocationChange")
EndFunction

Event Actor.OnLocationChange(Actor akSender, Location akOldLoc, Location akNewLoc)
	If IsStageDone(7200) || !IsStageDone(7100)
		UnregisterForRemoteEvent(akSender, "OnLocationChange")
		Return
	EndIf
	Location hideout = ExecHideoutLocation()
	If hideout != None && akNewLoc != None && (akNewLoc == hideout || akNewLoc.IsChild(hideout))
		UnregisterForRemoteEvent(akSender, "OnLocationChange")
		SetStage(7200)
	EndIf
EndEvent

Function Fragment_Stage_7200_Item_00()
	SetObjectiveCompleted(60, True)
EndFunction

Function Fragment_Stage_7300_Item_00()
	Actor execActor = Burn_SQ04_Exec_Killable.GetActorReference()
	If execActor != None
		execActor.SetProtected(False)
	EndIf
	SetObjectiveDisplayed(65, True)
EndFunction

Function Fragment_Stage_7400_Item_00()
	SetObjectiveCompleted(60, True)
	SetObjectiveDisplayed(65, False)
	SetObjectiveDisplayed(70, True)
EndFunction

Function Fragment_Stage_7450_Item_00()
	SetObjectiveCompleted(65, True)
	SetObjectiveDisplayed(70, True)
EndFunction

Function SetAliasAwayValue(Int aiAliasID, ActorValue akAwayValue)
	ReferenceAlias targetAlias = GetAlias(aiAliasID) as ReferenceAlias
	Actor targetActor = None
	If targetAlias != None
		targetActor = targetAlias.GetActorReference()
	EndIf
	If targetActor != None && akAwayValue != None
		targetActor.SetValue(akAwayValue, 1.0)
	EndIf
EndFunction

Function Fragment_Stage_8500_Item_00()
	SetAliasAwayValue(20, Burn_MQ_BodhiAwayValue)
	SetAliasAwayValue(40, Burn_MQ_ExecKillableAwayValue)
	SetAliasAwayValue(41, Burn_MQ_ExecAwayValue)
	; Bodhi's farewell line (8500) is the last step of "Return to Bodhi"; 9000 had no setter.
	If (IsStageDone(7400) || IsStageDone(7450)) && !IsStageDone(9000)
		SetStage(9000)
	EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
	CompleteAllObjectives()
EndFunction
