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

Function EnableAlias(ReferenceAlias aliasToEnable)
	If aliasToEnable != None
		aliasToEnable.TryToEnable()
	EndIf
EndFunction

Function DisableAlias(ReferenceAlias aliasToDisable)
	If aliasToDisable != None
		aliasToDisable.TryToDisable()
	EndIf
EndFunction

Function EvaluateAliasActor(ReferenceAlias actorAlias)
	If actorAlias != None
		Actor actorRef = actorAlias.GetActorReference()
		If actorRef != None
			actorRef.EvaluatePackage()
		EndIf
	EndIf
EndFunction

Function MoveAliasActor(ReferenceAlias actorAlias, ReferenceAlias markerAlias)
	If actorAlias == None || markerAlias == None
		Return
	EndIf
	Actor actorRef = actorAlias.GetActorReference()
	ObjectReference markerRef = markerAlias.GetReference()
	If actorRef != None && markerRef != None
		actorRef.MoveTo(markerRef)
		actorRef.EvaluatePackage()
	EndIf
EndFunction

Function StartBoundScene(Scene sceneToStart)
	If sceneToStart != None && !sceneToStart.IsPlaying()
		sceneToStart.Start()
	EndIf
EndFunction

Function StopBoundScene(Scene sceneToStop)
	If sceneToStop != None && sceneToStop.IsPlaying()
		sceneToStop.Stop()
	EndIf
EndFunction

Function SetArenaMusic(Bool shouldPlay)
	If Music_ArenaFight != None
		If shouldPlay
			Music_ArenaFight.Add()
		Else
			Music_ArenaFight.Remove()
		EndIf
	EndIf
EndFunction

Function AdvanceIfPending(Int nextStage)
	If !IsStageDone(nextStage)
		SetStage(nextStage)
	EndIf
EndFunction

Function StartArenaWave(Int waveIndex)
	DefaultQuestEncounterWaveScript encounterController = (Self as Quest) as DefaultQuestEncounterWaveScript
	If encounterController != None
		encounterController.StartLocalEncounterWave(waveIndex)
	EndIf
EndFunction

Function StopArenaWaveCombat()
	DefaultQuestEncounterWaveScript encounterController = (Self as Quest) as DefaultQuestEncounterWaveScript
	If encounterController != None
		encounterController.UnregisterLocalWaveCollection(Actors_EWS_Raiders)
	EndIf
	If Actors_EWS_Raiders != None
		Int actorIndex = 0
		While actorIndex < Actors_EWS_Raiders.GetCount()
			Actor arenaActor = Actors_EWS_Raiders.GetAt(actorIndex) as Actor
			If arenaActor != None && !arenaActor.IsDead()
				arenaActor.StopCombat()
			EndIf
			actorIndex += 1
		EndWhile
	EndIf
EndFunction

Function MakeEugeneHostile()
	Actor eugene = Actor_Eugene_Arena.GetActorReference()
	Actor playerRef = PlayerReference() as Actor
	If eugene == None
		Return
	EndIf
	If Faction_PlayerAllyFaction != None
		eugene.RemoveFromFaction(Faction_PlayerAllyFaction)
	EndIf
	If Faction_EugeneFaction != None
		eugene.RemoveFromFaction(Faction_EugeneFaction)
	EndIf
	If Faction_PlayerEnemyFaction != None && !eugene.IsInFaction(Faction_PlayerEnemyFaction)
		eugene.AddToFaction(Faction_PlayerEnemyFaction)
	EndIf
	eugene.SetGhost(False)
	eugene.SetProtected(False)
	If playerRef != None && !eugene.IsDead()
		eugene.StartCombat(playerRef)
	EndIf
EndFunction

Function Fragment_Stage_0005_Item_00()
	AdvanceIfPending(50)
EndFunction

Function Fragment_Stage_0050_Item_00()
	ObjectReference playerRef = PlayerReference()
	If playerRef != None && Key_Pipe != None && playerRef.GetItemCount(Key_Pipe) == 0
		playerRef.AddItem(Key_Pipe, 1, True)
	EndIf
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveCompleted(10)
	SetObjectiveDisplayed(11)
	SetPlayerValue(AV_EugeneBasement, 0.0)
	SetPlayerValue(AV_MagpieBasement, 0.0)
	SetPlayerValue(AV_RustKingEnable, 1.0)
	DisableAlias(Actor_Eugene_Basement)
	DisableAlias(Actor_Magpie_Basement)
	EnableAlias(Actor_Eugene_Arena)
	EnableAlias(Actor_RustKing)
	EnableAlias(Actor_Moose_Arena_Dead)
	EnableAlias(Actor_Leonard_Arena_Dead)
	EnableAlias(Actor_Magpie_Arena_Dead)
	EvaluateAliasActor(Actor_Eugene_Arena)
	EvaluateAliasActor(Actor_RustKing)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(11)
	SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0215_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveDisplayed(25)
EndFunction

Function Fragment_Stage_0225_Item_00()
	SetObjectiveCompleted(25)
	SetObjectiveDisplayed(30)
	SetPlayerValue(AV_RevealSceneCheck, 1.0)
	EnableAlias(EnableMarker_ArenaSpikes)
	SetArenaMusic(True)
	StartBoundScene(WaveStartScenes)
	StartArenaWave(0)
EndFunction

Function Fragment_Stage_0250_Item_00()
	StartArenaWave(1)
EndFunction

Function Fragment_Stage_0300_Item_00()
	StartArenaWave(2)
EndFunction

Function Fragment_Stage_0350_Item_00()
	StartArenaWave(3)
EndFunction

Function Fragment_Stage_0360_Item_00()
	StartArenaWave(4)
EndFunction

Function Fragment_Stage_0370_Item_00()
	AdvanceIfPending(400)
EndFunction

Function Fragment_Stage_0400_Item_00()
	AdvanceIfPending(500)
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveCompleted(30)
	SetObjectiveDisplayed(50)
	SetPlayerValue(AV_RevealSceneCheck, 2.0)
	StopBoundScene(WaveStartScenes)
	StopArenaWaveCombat()
	SetArenaMusic(False)
	MoveAliasActor(Actor_RustKing, Marker_RustKing_MoveTo)
	MoveAliasActor(Actor_Eugene_Arena, Marker_Eugene_MoveTo)
	StartBoundScene(SCENE_Eugene_WalkTo)
EndFunction

Function Fragment_Stage_0505_Item_00()
	Actor eugene = Actor_Eugene_Arena.GetActorReference()
	Actor rustKing = Actor_RustKing.GetActorReference()
	If eugene != None && !eugene.IsDead()
		eugene.SetGhost(False)
		eugene.SetProtected(False)
		eugene.Kill(rustKing)
	EndIf
	SetObjectiveCompleted(50)
	AdvanceIfPending(510)
EndFunction

Function Fragment_Stage_0506_Item_00()
	SetObjectiveCompleted(50)
	SetObjectiveDisplayed(60)
	MakeEugeneHostile()
EndFunction

Function Fragment_Stage_0507_Item_00()
	SetObjectiveCompleted(60)
	SetObjectiveDisplayed(70)
	AdvanceIfPending(510)
EndFunction

Function Fragment_Stage_0510_Item_00()
	If IsObjectiveDisplayed(70)
		SetObjectiveCompleted(70)
	EndIf
	EvaluateAliasActor(Actor_RustKing)
EndFunction

Function Fragment_Stage_0525_Item_00()
	StopBoundScene(SCENE_Eugene_WalkTo)
	EnableAlias(Alias_Furn_Knockout_Player)
	ObjectReference playerRef = PlayerReference()
	ObjectReference knockoutFurniture = Alias_Furn_Knockout_Player.GetReference()
	If playerRef != None && knockoutFurniture != None
		knockoutFurniture.Activate(playerRef)
	EndIf
	AdvanceIfPending(550)
EndFunction

Function Fragment_Stage_0550_Item_00()
	ObjectReference playerRef = PlayerReference()
	If FadeToBlackSpell != None && playerRef != None
		FadeToBlackSpell.Cast(playerRef, playerRef)
	EndIf
	AdvanceIfPending(560)
EndFunction

Function Fragment_Stage_0560_Item_00()
	ObjectReference playerRef = PlayerReference()
	ObjectReference teleportMarker = Marker_Teleport.GetReference()
	If playerRef != None && teleportMarker != None
		playerRef.MoveTo(teleportMarker)
	EndIf
	AdvanceIfPending(600)
EndFunction

Function Fragment_Stage_0600_Item_00()
	ObjectReference playerRef = PlayerReference()
	If playerRef != None && Key_Pipe != None
		Int keyCount = playerRef.GetItemCount(Key_Pipe)
		If keyCount > 0
			playerRef.RemoveItem(Key_Pipe, keyCount, True)
		EndIf
	EndIf
	DisableAlias(Alias_Furn_Knockout_Player)
	DisableAlias(EnableMarker_ArenaSpikes)
	SetArenaMusic(False)
	AdvanceIfPending(9000)
EndFunction

Function Fragment_Stage_9000_Item_00()
	CompleteAllObjectives()
	AdvanceIfPending(9999)
EndFunction

Function Fragment_Stage_9999_Item_00()
	StopBoundScene(WaveStartScenes)
	StopBoundScene(SCENE_Eugene_WalkTo)
	StopArenaWaveCombat()
	SetArenaMusic(False)
	Stop()
EndFunction
