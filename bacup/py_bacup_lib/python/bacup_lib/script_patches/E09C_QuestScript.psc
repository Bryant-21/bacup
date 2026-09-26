; Tunnel of Love (65B0A8) phase controller.
;
; QF_E09C_LoveTunnel_0065B0A8 owns the objective flow and consumes the four
; phase-completion stages. The phase boundaries are the record's bound values:
; iDecoStartStage 110 -> iDecoStageToSetOnComplete 190, iTrackStartStage 200 ->
; 290, iHandyStartStage 300 -> 390, iWeddingStartStage 400 -> 500.
;
; Each phase opens by placing its content (heart lamps at ItemSpawns, ten broken
; tracks out of PotentialBrokenTracks, the three robot parts at their spawn
; markers, the reception food) and completes on a world observable: lit
; decoration enablers, re-enabled track pieces, robot parts handed to Miss
; Lovely, and wedding merriment.

Int Function PhasePollTimerID() Global
	Return 4091
EndFunction

B21:B21_TFA_PublicEventController Function EmoteBus()
	Return Game.GetFormFromFile(0xFFF017, "B21_TalesFromAppalachia.esm") as B21:B21_TFA_PublicEventController
EndFunction

Function ArmEmoteListener()
	B21:B21_TFA_PublicEventController bus = EmoteBus()
	If bus != None
		RegisterForCustomEvent(bus, "B21EmoteV1")
	EndIf
EndFunction

Event OnQuestInit()
	ArmEmoteListener()
	RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
	cleanupDone = False
	decorationSpawns = New ObjectReference[0]
	B21WeddingMerriment = 0
	B21NextCalloutTime = 0.0
	ResetRobotPartTracking()
	PublishPhaseVariables()
	ArmPhasePoll()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID == iDecoStartStage || auiStageID == iTrackStartStage || auiStageID == iHandyStartStage || auiStageID == iWeddingStartStage
		SpawnPhaseContent(auiStageID)
		ArmPhasePoll()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID != PhasePollTimerID()
		Return
	EndIf
	If EvaluatePhases()
		ArmPhasePoll()
	EndIf
EndEvent

Event OnQuestShutdown()
	B21:B21_TFA_PublicEventController bus = EmoteBus()
	If bus != None
		UnregisterForCustomEvent(bus, "B21EmoteV1")
	EndIf
	UnregisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
	CancelTimer(PhasePollTimerID())
	ResetEventWorld()
EndEvent

Function ArmPhasePoll()
	CancelTimer(PhasePollTimerID())
	StartTimer(5.0, PhasePollTimerID())
EndFunction

Bool Function IsEventOver()
	Return IsStageDone(iWeddingStageToSetOnComplete) || IsStageDone(9000) || IsStageDone(9990) || IsCompleted() || IsStopping() || IsStopped()
EndFunction

Bool Function EvaluatePhases()
	; Returns whether the event is still in a phase worth polling, so a finished
	; or failed run stops re-arming instead of idling forever.
	If IsEventOver()
		Return False
	EndIf
	PublishPhaseVariables()

	If IsStageDone(iDecoStartStage) && !IsStageDone(iDecoStageToSetOnComplete)
		RunTimedCallouts(iDecoStageToSetOnComplete)
		EvaluateDecorations()
	ElseIf IsStageDone(iTrackStartStage) && !IsStageDone(iTrackStageToSetOnComplete)
		RunTimedCallouts(iTrackStageToSetOnComplete)
		EvaluateTracks()
	ElseIf IsStageDone(iHandyStartStage) && !IsStageDone(iHandyStageToSetOnComplete)
		RunTimedCallouts(iHandyStageToSetOnComplete)
		EvaluateHandy()
	EndIf
	Return True
EndFunction

Event Actor.OnPlayerLoadGame(Actor akSender)
	ArmEmoteListener()
EndEvent

Event B21:B21_TFA_PublicEventController.B21EmoteV1(B21:B21_TFA_PublicEventController akSender, Var[] akArgs)
	If akArgs.Length != 5
		Return
	EndIf
	Int aiVersion = akArgs[0] as Int
	Actor akPlayer = akArgs[1] as Actor
	String asPlugin = akArgs[2] as String
	Int aiSourceID = akArgs[3] as Int
	Int aiCategoryID = akArgs[4] as Int
	If aiVersion == 1 && akPlayer == Game.GetPlayer() && asPlugin == "SeventySix.esm" && aiCategoryID == 22337 && (aiSourceID == 1114476 || aiSourceID == 5234353) && PlayerAttendingWedding()
		AddWeddingMerriment(1)
	EndIf
EndEvent

Function CompletePhase(Int aiStage)
	B21NextCalloutTime = 0.0
	If !IsStageDone(aiStage)
		SetStage(aiStage)
	EndIf
EndFunction

Function EvaluateDecorations()
	If IsStageDone(iDecoStartStage) && !IsStageDone(iDecoStageToSetOnComplete) && CountEnabledAliasRefs(DecorationEnablers) >= iDecoObjectiveMax
		CompletePhase(iDecoStageToSetOnComplete)
	EndIf
EndFunction

Function EvaluateTracks()
	If IsStageDone(iTrackStartStage) && !IsStageDone(iTrackStageToSetOnComplete) && TrackGoal() > 0 && CountRepairedTracks() >= TrackGoal()
		CompletePhase(iTrackStageToSetOnComplete)
	EndIf
EndFunction

Function EvaluateHandy()
	If !IsStageDone(iHandyStartStage) || IsStageDone(iHandyStageToSetOnComplete)
		Return
	EndIf
	If CountDepositedRobotParts() >= iFakeHandyObjectiveMax
		; Stage 350 walks Mr. Lovely to the wedding before the build buffer stage.
		If !IsStageDone(350)
			SetStage(350)
		EndIf
		CompletePhase(iHandyStageToSetOnComplete)
	EndIf
EndFunction

Function DecorationPlaced()
	; Called by E09C_AliasTriggerScript (sFunctionToCall) on each decoration trigger.
	PublishPhaseVariables()
	EvaluateDecorations()
EndFunction

Function BuildHandy()
	; Called by E09C_AliasTriggerScript (sFunctionToCall) on Miss Lovely after a part is handed over.
	PublishPhaseVariables()
	EvaluateHandy()
EndFunction

Function TrackRepaired()
	PublishPhaseVariables()
	EvaluateTracks()
EndFunction

Function AddWeddingMerriment(Int aiAmount)
	If aiAmount <= 0 || !IsStageDone(iWeddingStartStage) || IsStageDone(iWeddingStageToSetOnComplete) || IsEventOver()
		Return
	EndIf
	B21WeddingMerriment += aiAmount
	If B21WeddingMerriment > iWeddingObjectiveMax
		B21WeddingMerriment = iWeddingObjectiveMax
	EndIf
	PublishPhaseVariables()
	If B21WeddingMerriment >= iWeddingObjectiveMax
		CompletePhase(iWeddingStageToSetOnComplete)
	EndIf
EndFunction

Function SpawnPhaseContent(Int aiStage)
	If aiStage == iDecoStartStage
		SpawnDecorations()
		StartPhaseWaves(1)
	ElseIf aiStage == iTrackStartStage
		BreakTracks()
		StartPhaseWaves(2)
	ElseIf aiStage == iHandyStartStage
		SpawnRobotParts()
		StartPhaseWaves(3)
		StartDeathclawBoss()
	ElseIf aiStage == iWeddingStartStage
		SpawnWeddingFood()
	EndIf
	PublishPhaseVariables()
EndFunction

Int[] Function ShuffledIndices(Int aiCount, Int aiWanted)
	Int[] candidates = New Int[0]
	Int index = 0
	While index < aiCount
		candidates.Add(index)
		index += 1
	EndWhile
	Int[] picks = New Int[0]
	While picks.Length < aiWanted && candidates.Length > 0
		Int pick = Utility.RandomInt(0, candidates.Length - 1)
		picks.Add(candidates[pick])
		candidates.Remove(pick)
	EndWhile
	Return picks
EndFunction

Function TrackSpawn(ObjectReference akSpawn)
	If akSpawn == None
		Return
	EndIf
	If decorationSpawns == None
		decorationSpawns = New ObjectReference[0]
	EndIf
	decorationSpawns.Add(akSpawn)
EndFunction

Function SpawnDecorations()
	If Alias_ItemSpawns == None || Form_DecorationBox == None || DecorationBoxes == None || DecorationBoxes.GetCount() > 0
		Return
	EndIf
	Int wanted = iDecoAmountOverrive
	If wanted <= 0
		wanted = iDecoObjectiveMax
	EndIf
	Int[] picks = ShuffledIndices(Alias_ItemSpawns.GetCount(), wanted)
	Int index = 0
	While index < picks.Length
		ObjectReference spawnPoint = Alias_ItemSpawns.GetAt(picks[index])
		If spawnPoint != None
			ObjectReference lamp = spawnPoint.PlaceAtMe(Form_DecorationBox, 1, True, False, False)
			If lamp != None
				DecorationBoxes.AddRef(lamp)
				TrackSpawn(lamp)
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction

Function BreakTracks()
	If TrackRefCollection == None || BrokenTrackRefCollection == None || BrokenTrackRefCollection.GetCount() > 0
		Return
	EndIf
	; Each potential track piece links to an invisible "Broken Track" activator whose enable parent is the
	; piece with the opposite state: disabling a piece exposes its repair activator.
	Int[] picks = ShuffledIndices(TrackRefCollection.GetCount(), iTracksObjectiveMax)
	Int index = 0
	While index < picks.Length
		ObjectReference piece = TrackRefCollection.GetAt(picks[index])
		If piece != None
			ObjectReference repairActivator = piece.GetLinkedRef()
			If repairActivator != None
				piece.Disable(False)
				BrokenTrackRefCollection.AddRef(repairActivator)
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction

Function SpawnRobotParts()
	ResetRobotPartTracking()
	If Form_RobotParts == None || RobotPartSpawns == None || (RobotParts != None && RobotParts.GetCount() > 0)
		Return
	EndIf
	Int index = 0
	While index < Form_RobotParts.Length && index < RobotPartSpawns.Length
		ObjectReference spawnPoint = None
		If RobotPartSpawns[index] != None
			spawnPoint = RobotPartSpawns[index].GetReference()
		EndIf
		If spawnPoint != None && Form_RobotParts[index] != None
			ObjectReference part = spawnPoint.PlaceAtMe(Form_RobotParts[index], 1, True, False, False)
			If part != None
				If RobotPartAliases != None && index < RobotPartAliases.Length && RobotPartAliases[index] != None
					RobotPartAliases[index].ForceRefTo(part)
				EndIf
				If RobotParts != None
					RobotParts.AddRef(part)
				EndIf
				TrackSpawn(part)
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction

Function SpawnWeddingFood()
	If WeddingFoodSpawns == None || E09C_LL_WeddingFood == None
		Return
	EndIf
	Int index = 0
	While index < WeddingFoodSpawns.GetCount()
		ObjectReference spawnPoint = WeddingFoodSpawns.GetAt(index)
		If spawnPoint != None
			TrackSpawn(spawnPoint.PlaceAtMe(E09C_LL_WeddingFood, 1, False, False, True))
		EndIf
		index += 1
	EndWhile
EndFunction

Function StartPhaseWaves(Int aiPhase)
	Quest owner = Self as Quest
	DefaultQuestEncounterWaveScript waves = owner as DefaultQuestEncounterWaveScript
	If waves == None || EWSMobWaves == None
		Return
	EndIf
	; EWSMobWaves maps each EncounterWaves row to its phase (ObjectiveStage 1 decorations, 2 tracks, 3 Miss Lovely).
	Int index = 0
	While index < EWSMobWaves.Length
		EWS row = EWSMobWaves[index]
		If row != None && row.ObjectiveStage == aiPhase
			waves.StartEncounterWave(row.EWSWave)
		EndIf
		index += 1
	EndWhile
EndFunction

Function StartDeathclawBoss()
	Quest owner = Self as Quest
	DefaultQuestEncounterWaveScript waves = owner as DefaultQuestEncounterWaveScript
	If waves == None
		Return
	EndIf
	; The two boss rows are the same legendary deathclaw at either nest; one appears per run.
	If Utility.RandomInt(0, 1) == 0
		waves.StartEncounterWaveByID("DeathclawBoss_Lower")
	Else
		waves.StartEncounterWaveByID("DeathclawBoss_Upper")
	EndIf
EndFunction

Int Function CountEnabledAliasRefs(referencealias[] akAliases)
	If akAliases == None
		Return 0
	EndIf
	Int enabled = 0
	Int index = 0
	While index < akAliases.Length
		referencealias entry = akAliases[index]
		If entry != None
			ObjectReference decoration = entry.GetReference()
			If decoration != None && !decoration.IsDisabled()
				enabled += 1
			EndIf
		EndIf
		index += 1
	EndWhile
	Return enabled
EndFunction

Int Function TrackGoal()
	If BrokenTrackRefCollection == None
		Return 0
	EndIf
	Int broken = BrokenTrackRefCollection.GetCount()
	If broken > iTracksObjectiveMax
		Return iTracksObjectiveMax
	EndIf
	Return broken
EndFunction

Int Function CountRepairedTracks()
	If BrokenTrackRefCollection == None
		Return 0
	EndIf
	Int repaired = 0
	Int index = 0
	While index < BrokenTrackRefCollection.GetCount()
		ObjectReference repairActivator = BrokenTrackRefCollection.GetAt(index)
		If repairActivator != None
			ObjectReference piece = repairActivator.GetLinkedRef()
			If piece != None && !piece.IsDisabled()
				repaired += 1
			EndIf
		EndIf
		index += 1
	EndWhile
	Return repaired
EndFunction

Int Function CountDepositedRobotParts()
	; E09C_PlayerAliasScript raises each part's tracking actor value when the part leaves the player for Miss Lovely.
	Actor player = Game.GetPlayer()
	If player == None || RobotPartTrackingAVs == None
		Return 0
	EndIf
	Int deposited = 0
	Int index = 0
	While index < RobotPartTrackingAVs.Length
		If RobotPartTrackingAVs[index] != None && player.GetValue(RobotPartTrackingAVs[index]) > 0.0
			deposited += 1
		EndIf
		index += 1
	EndWhile
	Return deposited
EndFunction

Function ResetRobotPartTracking()
	Actor player = Game.GetPlayer()
	If player == None || RobotPartTrackingAVs == None
		Return
	EndIf
	Int index = 0
	While index < RobotPartTrackingAVs.Length
		If RobotPartTrackingAVs[index] != None
			player.SetValue(RobotPartTrackingAVs[index], 0.0)
		EndIf
		index += 1
	EndWhile
EndFunction

Bool Function PlayerAttendingWedding()
	Actor player = Game.GetPlayer()
	If player == None || FakeMissHandy == None
		Return False
	EndIf
	ObjectReference bride = FakeMissHandy.GetReference()
	If bride == None
		Return False
	EndIf
	; Alias 1 (players) carries the wedding radius, which the record matches to the objective radius.
	Float radius = 1536.0
	E09C_PlayerAliasScript players = GetAlias(1) as E09C_PlayerAliasScript
	If players != None && players.fDistanceToWedding > 0.0
		radius = players.fDistanceToWedding
	EndIf
	Return player.GetDistance(bride) <= radius
EndFunction

Function PublishPhaseVariables()
	Quest owner = Self as Quest
	B21:QuestVariables questVariables = owner as B21:QuestVariables
	If questVariables == None
		Return
	EndIf
	questVariables.SetVariable("DecoCount", CountEnabledAliasRefs(DecorationEnablers) as Float)
	questVariables.SetVariable("DecoMax", iDecoObjectiveMax as Float)
	questVariables.SetVariable("TracksCount", CountRepairedTracks() as Float)
	Int trackGoal = TrackGoal()
	If trackGoal <= 0
		trackGoal = iTracksObjectiveMax
	EndIf
	questVariables.SetVariable("TracksMax", trackGoal as Float)
	questVariables.SetVariable("FakeHandyCount", CountDepositedRobotParts() as Float)
	questVariables.SetVariable("FakeHandyMax", iFakeHandyObjectiveMax as Float)
	questVariables.SetVariable("WeddingCount", B21WeddingMerriment as Float)
	questVariables.SetVariable("WeddingMax", iWeddingObjectiveMax as Float)
EndFunction

Function RunTimedCallouts(Int aiPhaseStopStage)
	; A callout replays its reminder scene every fTimerInterval seconds from iStageToStart until iStageToStop.
	If TimedObjectiveCallouts == None
		Return
	EndIf
	Int index = 0
	While index < TimedObjectiveCallouts.Length
		ObjectiveCallout callout = TimedObjectiveCallouts[index]
		If callout != None && callout.iStageToStop == aiPhaseStopStage && IsStageDone(callout.iStageToStart) && !IsStageDone(callout.iStageToStop)
			Float now = Utility.GetCurrentRealTime()
			; Real time restarts with the game, so a deadline further out than one interval came from another session.
			If B21NextCalloutTime <= 0.0 || B21NextCalloutTime - now > callout.fTimerInterval
				B21NextCalloutTime = now + callout.fTimerInterval
			ElseIf now >= B21NextCalloutTime
				B21NextCalloutTime = now + callout.fTimerInterval
				If callout.pSceneToStart != None && !callout.pSceneToStart.IsPlaying()
					callout.pSceneToStart.Start()
				EndIf
			EndIf
			Return
		EndIf
		index += 1
	EndWhile
EndFunction

Function ResetEventWorld()
	If cleanupDone
		Return
	EndIf
	cleanupDone = True

	Int index = 0
	While DecorationEnablers != None && index < DecorationEnablers.Length
		If DecorationEnablers[index] != None && DecorationEnablers[index].GetReference() != None
			DecorationEnablers[index].GetReference().Disable(False)
		EndIf
		index += 1
	EndWhile
	index = 0
	While DecorationTriggers != None && index < DecorationTriggers.Length
		If DecorationTriggers[index] != None && DecorationTriggers[index].GetReference() != None
			DecorationTriggers[index].GetReference().Enable(False)
		EndIf
		index += 1
	EndWhile

	index = 0
	While TrackRefCollection != None && index < TrackRefCollection.GetCount()
		ObjectReference piece = TrackRefCollection.GetAt(index)
		If piece != None && piece.IsDisabled()
			piece.Enable(False)
		EndIf
		index += 1
	EndWhile
	If BrokenTrackRefCollection != None
		BrokenTrackRefCollection.RemoveAll()
	EndIf

	index = 0
	While decorationSpawns != None && index < decorationSpawns.Length
		ObjectReference spawned = decorationSpawns[index]
		; Carried items belong to the player cleanup on QO_Items; only world copies are removed here.
		If spawned != None && !spawned.IsDeleted() && spawned.GetContainer() == None
			spawned.Disable(False)
			spawned.Delete()
		EndIf
		index += 1
	EndWhile
	decorationSpawns = New ObjectReference[0]
	If DecorationBoxes != None
		DecorationBoxes.RemoveAll()
	EndIf
	If RobotParts != None
		RobotParts.RemoveAll()
	EndIf
	index = 0
	While RobotPartAliases != None && index < RobotPartAliases.Length
		If RobotPartAliases[index] != None
			RobotPartAliases[index].Clear()
		EndIf
		index += 1
	EndWhile
	ResetRobotPartTracking()
	B21WeddingMerriment = 0
	B21NextCalloutTime = 0.0

	If Alias_MrHandy != None
		E09C_TOLHandyScript mrLovely = Alias_MrHandy.GetActorReference() as E09C_TOLHandyScript
		If mrLovely != None && mrLovely.ResetLocation != None
			mrLovely.MoveTo(mrLovely.ResetLocation)
		EndIf
	EndIf
EndFunction
