; FO76's server picked the boss's creature, spawn point and patrol edges. Tales' roaming pulse
; passes them through B21BeginRoaming; a quest started any other way configures itself near the player.
Event OnQuestInit()
    StartTimer(10.0, 7600)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 7600 && EncounterWaveIns == None && IsRunning()
        Actor playerRef = Game.GetPlayer()
        ObjectReference anchor = B21:EncounterWaveCatalog.PlaceRoamingMarker(playerRef, 3000.0)
        If !B21BeginRoaming(None, anchor, playerRef.GetCurrentLocation(), Utility.RandomInt(1, 3), 0)
            If anchor != None
                anchor.Delete()
            EndIf
            Stop()
        EndIf
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(7600)
    EncounterWaveIns = None
EndEvent

Bool Function B21BeginRoaming(Keyword akCreature, ObjectReference akAnchor, Location akRegion, Int aiBossRank, Int aiWaves)
    If EncounterWaveIns != None || akAnchor == None
        Return False
    EndIf
    Quest owner = Self as Quest
    DefaultQuestEncounterWaveScript waves = owner as DefaultQuestEncounterWaveScript
    B21:EncounterWaveCatalog catalog = owner as B21:EncounterWaveCatalog
    If waves == None || catalog == None || waves.EncounterWaves == None || waves.EncounterWaves.Length == 0
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

    ReferenceAlias randomPlayer = GetAlias(0) as ReferenceAlias
    If randomPlayer != None
        randomPlayer.ForceRefTo(Game.GetPlayer())
    EndIf
    LocationAlias spawnRegion = GetAlias(2) as LocationAlias
    If spawnRegion != None && akRegion != None
        spawnRegion.ForceLocationTo(akRegion)
    EndIf
    SpawnNearRef.ForceRefTo(akAnchor)
    Int edge = 0
    While edge < 3
        ObjectReference edgeMarker = B21:EncounterWaveCatalog.PlaceRoamingMarker(akAnchor, 2500.0)
        If edgeMarker != None
            EdgeMarkers.AddRef(edgeMarker)
            edgeMarker.Delete()
        EndIf
        edge += 1
    EndWhile
    akAnchor.Delete()

    Int rank = aiBossRank
    If rank < 1
        rank = 1
    ElseIf rank > 5
        rank = 5
    EndIf
    DefaultQuestEncounterWaveScript:EncounterWaveData bossWaveData = waves.EncounterWaves[0]
    bossWaveData.EncounterTypeKeyword = creature
    bossWaveData.ManuallyOverrideLegendaryCreatureGeneration = True
    bossWaveData.DesiredMinNumLegendaryCreatures = 1
    bossWaveData.DesiredMaxNumLegendaryCreatures = 1
    bossWaveData.MinLegendaryCreatureRank = rank
    bossWaveData.MaxLegendaryCreatureRank = rank
    waves.StartEncounterWave(0)

    Actor bossActor = BossWave.GetAt(0) as Actor
    If bossActor == None
        Return False
    EndIf
    Boss.ForceRefTo(bossActor)
    ; The wave starts combat with a nearby player; a wandering boss patrols its edges until it notices them.
    bossActor.StopCombat()
    bossActor.EvaluatePackage()
    Return True
EndFunction

ObjectReference Function B21RoamingAnchor()
    ObjectReference bossRef = Boss.GetReference()
    If bossRef != None && !(bossRef as Actor).IsDead()
        Return bossRef
    EndIf
    Return SpawnNearRef.GetReference()
EndFunction
