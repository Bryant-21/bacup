; FO76's server rolled the horde's creature and decoy dungeons. The conversion runs the horde in its
; skip-dungeon form: Tales' roaming pulse passes the creature, spawn anchor and region through
; B21BeginRoaming, and a horde started any other way configures itself near the player.
Event OnQuestInit()
    StartTimer(10.0, 7600)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 7600 && EncounterWaveIns == None && IsRunning()
        Actor playerRef = Game.GetPlayer()
        ObjectReference anchor = B21:EncounterWaveCatalog.PlaceRoamingMarker(playerRef, 3000.0)
        If !B21BeginRoaming(None, anchor, Alias_Region.GetLocation(), Utility.RandomInt(1, 3), 0)
            If anchor != None
                anchor.Delete()
            EndIf
            Stop()
        EndIf
    ElseIf aiTimerID == TimerID_SpawnHorde && EncounterWaveIns != None
        B21HordeTick()
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(7600)
    CancelTimer(TimerID_SpawnHorde)
    If AlphaRef != None
        UnregisterForRemoteEvent(AlphaRef, "OnDeath")
    EndIf
    AlphaRef = None
    EncounterWaveIns = None
    SwarmInstanceCount = 0
EndEvent

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    If akSender == AlphaRef
        UnregisterForRemoteEvent(akSender, "OnDeath")
        If !IsStageDone(FoundAlphaStage)
            SetStage(FoundAlphaStage)
        EndIf
        SetStage(KilledAlphaStage)
    EndIf
EndEvent

Bool Function B21BeginRoaming(Keyword akCreature, ObjectReference akAnchor, Location akRegion, Int aiBossRank, Int aiWaves)
    If EncounterWaveIns != None || akAnchor == None
        Return False
    EndIf
    Quest owner = Self as Quest
    DefaultQuestEncounterWaveScript waves = owner as DefaultQuestEncounterWaveScript
    B21:EncounterWaveCatalog catalog = owner as B21:EncounterWaveCatalog
    If waves == None || catalog == None || waves.EncounterWaves == None || waves.EncounterWaves.Length <= iHordeWaveIndex
        Return False
    EndIf
    Keyword creature = akCreature
    If catalog.KeywordVariantCount(creature) <= 0
        creature = catalog.RandomPoolKeyword()
    EndIf
    If creature == None
        Return False
    EndIf
    EncounterWaveIns = waves
    SwarmInstanceCount = 0

    SkippedDungeon = True
    If akRegion != None && Alias_Region.GetLocation() == None
        Alias_Region.ForceLocationTo(akRegion)
    EndIf
    RegionLoc = Alias_Region.GetLocation()
    AlphaSpawnMarker.ForceRefTo(akAnchor)
    EventStartMarker.ForceRefTo(akAnchor)
    EventStartMarkerSkipDungeon.ForceRefTo(akAnchor)
    akAnchor.Delete()

    Int rank = aiBossRank
    If rank < 1
        rank = 1
    ElseIf rank > 5
        rank = 5
    EndIf
    Int waveIndex = 0
    While waveIndex < waves.EncounterWaves.Length
        DefaultQuestEncounterWaveScript:EncounterWaveData waveData = waves.EncounterWaves[waveIndex]
        waveData.EncounterTypeKeyword = creature
        If waveIndex == iAlphaWaveIndex
            waveData.ManuallyOverrideLegendaryCreatureGeneration = True
            waveData.DesiredMinNumLegendaryCreatures = 1
            waveData.DesiredMaxNumLegendaryCreatures = 1
            waveData.MinLegendaryCreatureRank = rank
            waveData.MaxLegendaryCreatureRank = rank
        EndIf
        waveIndex += 1
    EndWhile

    waves.StartEncounterWave(iAlphaWaveIndex)
    AlphaRef = Alias_Alpha.GetAt(0) as Actor
    If AlphaRef == None
        Return False
    EndIf
    Alias_AlphaRef.ForceRefTo(AlphaRef)
    RegisterForRemoteEvent(AlphaRef, "OnDeath")
    waves.StartEncounterWave(iEntourageWaveIndex)
    waves.StartEncounterWave(iHordeWaveIndex)

    SetStage(InitializationComplete)
    SetStage(EventStartedStage)
    StartTimer(5.0, TimerID_SpawnHorde)
    Return True
EndFunction

; Before the kill: the alpha counts as found once the player is close or it is fighting.
; After the kill: the quest stops once the horde is dead or the player has left, which removes leftovers.
Function B21HordeTick()
    Actor playerRef = Game.GetPlayer()
    If !IsStageDone(KilledAlphaStage)
        If !IsStageDone(FoundAlphaStage) && AlphaRef != None && (playerRef.GetDistance(AlphaRef) <= DiscoveryDistance || AlphaRef.IsInCombat())
            SetStage(FoundAlphaStage)
        EndIf
        StartTimer(5.0, TimerID_SpawnHorde)
        Return
    EndIf
    SwarmInstanceCount += 1
    Int living = EncounterWaveIns.CountLivingWaveActors(iEntourageWaveIndex) + EncounterWaveIns.CountLivingWaveActors(iHordeWaveIndex)
    ObjectReference anchor = EventStartMarker.GetReference()
    If living <= 0 || SwarmInstanceCount >= 120 || anchor == None || playerRef.GetDistance(anchor) > 8000.0
        Stop()
    Else
        StartTimer(5.0, TimerID_SpawnHorde)
    EndIf
EndFunction

ObjectReference Function B21RoamingAnchor()
    If AlphaRef != None && !AlphaRef.IsDead()
        Return AlphaRef
    EndIf
    Return EventStartMarker.GetReference()
EndFunction
