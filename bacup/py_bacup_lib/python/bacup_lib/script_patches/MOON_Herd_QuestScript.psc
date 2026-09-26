Event OnQuestInit()
    EWS = None
    BrahminCollection_External = None
    B21TalkNPC = None
    B21TalkStage = -1
    B21PendingScene = None
    B21PendingSceneStage = -1
    B21PendingSceneSeconds = 0.0
EndEvent

Event OnQuestShutdown()
    CancelEventTimers()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If iScreenEffectStages != None && iScreenEffectStages.Find(auiStageID) >= 0
        PlayScreenEffect()
    EndIf
    If auiStageID == iCircuitBreakerFailsafeStartStage
        StartTimer(fCircuitBreakerFailsafeTimer, iCircuitBreakerFailsafeTimerID)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == iBDSpawnTimerID
        If IsRunning() && !IsStageDone(950) && iBDSpawnStageToSet >= 0
            SetStage(iBDSpawnStageToSet)
        EndIf
    ElseIf aiTimerID == iCircuitBreakerFailsafeTimerID
        CheckCircuitBreakers()
    ElseIf aiTimerID == 69065
        TickTalkFallback()
    ElseIf aiTimerID == 69066
        TickPendingScene()
    EndIf
EndEvent

Function CancelEventTimers()
    CancelTimer(iBDSpawnTimerID)
    CancelTimer(iCircuitBreakerFailsafeTimerID)
    CancelTimer(69065)
    CancelTimer(69066)
EndFunction

defaultquestencounterwavescript Function EventWaves()
    If EWS == None
        Quest owner = Self as Quest
        EWS = owner as defaultquestencounterwavescript
    EndIf
    Return EWS
EndFunction

Function StartEventWave(String asWaveID)
    defaultquestencounterwavescript waves = EventWaves()
    If waves != None
        waves.StartEncounterWaveByID(asWaveID)
    EndIf
EndFunction

Function StopEventWave(String asWaveID)
    defaultquestencounterwavescript waves = EventWaves()
    If waves != None
        waves.StopEncounterWaveByID(asWaveID, False)
    EndIf
EndFunction

Function StopAllEventWaves(Bool abRemoveActors)
    defaultquestencounterwavescript waves = EventWaves()
    If waves != None
        waves.StopAllEncounterWaves(abRemoveActors)
    EndIf
EndFunction

Function InitializeEvent()
    ResetObjectiveTextVariables()
    SetCircuitBreakersFixed(False)
    GatherPitstopBrahmin()
    MoveEventActors(True)
EndFunction

Function CleanupEvent()
    CancelEventTimers()
    EndTalkFallback()
    B21PendingScene = None
    B21PendingSceneStage = -1
    StopAllEventWaves(True)
    SetCircuitBreakersFixed(True)
    ResetObjectiveTextVariables()
    MoveEventActors(False)
EndFunction

Function ResetObjectiveTextVariables()
    Quest owner = Self as Quest
    B21:QuestVariables variables = owner as B21:QuestVariables
    If variables == None || ObjectiveTextVariables == None
        Return
    EndIf
    Int index = 0
    While index < ObjectiveTextVariables.Length
        TextVariable entry = ObjectiveTextVariables[index]
        If entry != None
            variables.SetVariable(entry.variableName, entry.initialValue)
        EndIf
        index += 1
    EndWhile
EndFunction

Function SetTextVariableForCollection(RefCollectionAlias akCollection, Int aiActivations)
    Quest owner = Self as Quest
    B21:QuestVariables variables = owner as B21:QuestVariables
    If variables == None || ObjectiveTextVariables == None || akCollection == None
        Return
    EndIf
    Int index = 0
    While index < ObjectiveTextVariables.Length
        TextVariable entry = ObjectiveTextVariables[index]
        If entry != None && entry.collectionAliasToWatch == akCollection
            variables.SetVariable(entry.variableName, entry.initialValue + (aiActivations * entry.incrementOnActivate) as Float)
        EndIf
        index += 1
    EndWhile
EndFunction

Function SetCircuitBreakersFixed(Bool abFixed)
    Moon_Herd_PowerBoxAliasScript powerBoxes = Alias_PowerBoxes as Moon_Herd_PowerBoxAliasScript
    If powerBoxes != None
        powerBoxes.SetAllPowerBoxesFixed(abFixed)
    EndIf
EndFunction

Function CheckCircuitBreakers()
    If !IsRunning() || !IsStageDone(iCircuitBreakerFailsafeStartStage) || IsStageDone(iCircuitBreakerFailsafeEndStage)
        Return
    EndIf
    Moon_Herd_PowerBoxAliasScript powerBoxes = Alias_PowerBoxes as Moon_Herd_PowerBoxAliasScript
    If powerBoxes != None && powerBoxes.GetCount() > 0 && powerBoxes.CountFixedPowerBoxes() >= powerBoxes.GetCount()
        SetStage(iCircuitBreakerFailsafeEndStage)
        Return
    EndIf
    StartTimer(fCircuitBreakerFailsafeTimer, iCircuitBreakerFailsafeTimerID)
EndFunction

; The Pitstop brahmin live in MOON_MMPitstop_Dialogue's Actors_Brahmin alias (ID 8).
Function GatherPitstopBrahmin()
    If MOON_MMPitstop_Dialogue == None || BrahminCollection == None
        Return
    EndIf
    BrahminCollection_External = MOON_MMPitstop_Dialogue.GetAlias(8) as RefCollectionAlias
    If BrahminCollection_External != None && BrahminCollection_External.GetCount() > 0
        BrahminCollection.AddRefCollection(BrahminCollection_External)
    EndIf
EndFunction

Function MoveEventActors(Bool abToStart)
    Int index = 0
    While NPCSpawnLocations != None && index < NPCSpawnLocations.Length
        NPCLocations entry = NPCSpawnLocations[index]
        If entry != None && entry.NPC != None
            ReferenceAlias markerAlias = entry.EndMarker
            If abToStart
                markerAlias = entry.StartMarker
            EndIf
            ObjectReference npcRef = entry.NPC.GetReference()
            If npcRef != None && markerAlias != None && markerAlias.GetReference() != None
                npcRef.MoveTo(markerAlias.GetReference())
            EndIf
        EndIf
        index += 1
    EndWhile

    index = 0
    While BrahminCollection != None && index < BrahminCollection.GetCount()
        ObjectReference brahmin = BrahminCollection.GetAt(index)
        ObjectReference marker = BrahminMarker(brahmin, abToStart)
        If brahmin != None && marker != None
            brahmin.MoveTo(marker)
        EndIf
        index += 1
    EndWhile
EndFunction

ObjectReference Function BrahminMarker(ObjectReference akBrahmin, Bool abStart)
    If akBrahmin == None || BrahminSpawns == None
        Return None
    EndIf
    Int index = 0
    While index < BrahminSpawns.Length
        BrahminSpawn entry = BrahminSpawns[index]
        If entry != None && entry.Brahmin != None && akBrahmin.GetBaseObject() == entry.Brahmin
            If abStart
                Return entry.StartMarker
            EndIf
            Return entry.EndMarker
        EndIf
        index += 1
    EndWhile
    Return None
EndFunction

Function PlayScreenEffect()
    Actor playerRef = Game.GetPlayer()
    If ScreenEffectSpell != None && playerRef != None
        ScreenEffectSpell.Cast(playerRef, playerRef)
    EndIf
EndFunction

; The repeller pulse pops the heads of every critter still fighting at the Pitstop.
Function PulseRepellerOnEnemies(RefCollectionAlias akEnemies)
    Int index = 0
    If akEnemies != None
        index = akEnemies.GetCount() - 1
    EndIf
    While akEnemies != None && index >= 0
        Actor enemy = akEnemies.GetAt(index) as Actor
        If enemy != None && !enemy.IsDead()
            If Moon_Herd_DismemberActorSpell != None
                Moon_Herd_DismemberActorSpell.Cast(enemy, enemy)
            EndIf
            enemy.Kill(None)
        EndIf
        index -= 1
    EndWhile
EndFunction

Function ScheduleBlueDevil()
    StartTimer(fBDSpawnTimerInterval, iBDSpawnTimerID)
EndFunction

; Vera's greeting starts MOON_Herd_Vera_Hub, whose "[Start Event]" reply sets the talk
; stage. When activating Vera plays no scene, the talk counts as accepted.
Function BeginTalkFallback(ReferenceAlias akSpeaker, Int aiTalkStage)
    EndTalkFallback()
    B21TalkNPC = akSpeaker
    B21TalkStage = aiTalkStage
    If akSpeaker != None && akSpeaker.GetReference() != None
        RegisterForRemoteEvent(akSpeaker.GetReference(), "OnActivate")
    EndIf
EndFunction

Function EndTalkFallback()
    CancelTimer(69065)
    If B21TalkNPC != None && B21TalkNPC.GetReference() != None
        UnregisterForRemoteEvent(B21TalkNPC.GetReference(), "OnActivate")
    EndIf
    B21TalkNPC = None
    B21TalkStage = -1
EndFunction

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    If B21TalkStage < 0 || akActionRef != Game.GetPlayer() || IsStageDone(B21TalkStage)
        Return
    EndIf
    StartTimer(2.0, 69065)
EndEvent

Function TickTalkFallback()
    If B21TalkStage < 0 || !IsRunning()
        Return
    EndIf
    If IsStageDone(B21TalkStage)
        EndTalkFallback()
        Return
    EndIf
    If B21TalkNPC != None && B21TalkNPC.GetReference() != None && B21TalkNPC.GetReference().GetCurrentScene() != None
        Return
    EndIf
    Int talkStage = B21TalkStage
    EndTalkFallback()
    SetStage(talkStage)
EndFunction

; Vera's closing radio scenes carried no completion stage; a silent scene advances at
; once, a playing one is capped. A None scene waits the full delay.
Function SetStageAfterScene(Scene akScene, Int aiStage, Float afMaxSeconds)
    If aiStage < 0 || IsStageDone(aiStage)
        Return
    EndIf
    B21PendingScene = akScene
    B21PendingSceneStage = aiStage
    B21PendingSceneSeconds = afMaxSeconds
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
    StartTimer(1.0, 69066)
EndFunction

Function TickPendingScene()
    If B21PendingSceneStage < 0 || !IsRunning()
        Return
    EndIf
    If IsStageDone(B21PendingSceneStage)
        B21PendingScene = None
        B21PendingSceneStage = -1
        Return
    EndIf
    B21PendingSceneSeconds -= 1.0
    Bool sceneFinished = B21PendingScene != None && !B21PendingScene.IsPlaying()
    If sceneFinished || B21PendingSceneSeconds <= 0.0
        Int pendingStage = B21PendingSceneStage
        B21PendingScene = None
        B21PendingSceneStage = -1
        SetStage(pendingStage)
        Return
    EndIf
    StartTimer(1.0, 69066)
EndFunction
