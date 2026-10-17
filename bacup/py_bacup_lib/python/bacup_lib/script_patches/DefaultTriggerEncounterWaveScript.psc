; FO76 started one DefaultTriggerEncounterWaveQuest instance per trigger. FO4 runs a quest once at a
; time, so the trigger spawns its own wave from the keyword pool the converter attaches to that quest.
; The script stays in the empty state; the parent's waiting/done story-event path is never entered.
Event OnTriggerEnter(ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If akActionRef != playerRef
        Return
    EndIf
    If B21WaveFired && !TriggerWaveResetElapsed()
        Return
    EndIf
    B21WaveFired = True
    B21WaveEndedDay = 0.0
    If !SpawnTriggerWave(playerRef)
        B21WaveFired = False
    EndIf
EndEvent

Bool Function SpawnTriggerWave(Actor akPlayer)
    B21:EncounterWaveCatalog catalog = Game.GetFormFromFile(0x0018B8EA, "SeventySix.esm") as B21:EncounterWaveCatalog
    If catalog == None
        Return False
    EndIf
    Keyword creature = ResolveTriggerWaveKeyword(catalog, akPlayer)
    Int variants = catalog.KeywordVariantCount(creature)
    If variants <= 0
        Return False
    EndIf

    ObjectReference center = GetLinkedRef()
    If center == None
        center = Self
    EndIf
    ; The linked SpawnCenter_MinX_MaxY markers hold their radii in data FO4 does not keep. Their bands span
    ; 0-2048 and most end at 1536.
    Float minDistance = SpawnAreaRadiusMin
    If minDistance < 0.0
        minDistance = 512.0
    EndIf
    Float maxDistance = SpawnAreaRadiusMax
    If maxDistance < minDistance
        maxDistance = minDistance + 1024.0
    EndIf

    Int count = TriggerWaveActorCount()
    Int variant = Utility.RandomInt(0, variants - 1)
    Actor[] spawned = New Actor[0]
    Int actorIndex = 0
    While actorIndex < count
        Form spawnForm = catalog.PickKeywordCandidate(creature, variant, actorIndex)
        ObjectReference marker = B21:EncounterWaveCatalog.PlaceRoamingMarker(center, Utility.RandomFloat(minDistance, maxDistance))
        If spawnForm != None && marker != None
            Actor waveActor = marker.PlaceAtMe(spawnForm, 1, False, True, False) as Actor
            If waveActor != None
                spawned.Add(waveActor)
                RegisterForRemoteEvent(waveActor, "OnDeath")
                waveActor.EnableNoWait()
                waveActor.StartCombat(akPlayer)
            EndIf
        EndIf
        If marker != None
            marker.Delete()
        EndIf
        actorIndex += 1
    EndWhile
    If spawned.Length == 0
        Return False
    EndIf

    ; The cell can unload during the spawn's latent calls, and a wave actor can die before it is tracked.
    Cell parentCell = GetParentCell()
    If parentCell == None || !parentCell.IsAttached()
        actorIndex = 0
        While actorIndex < spawned.Length
            RemoveTriggerWaveActor(spawned[actorIndex])
            actorIndex += 1
        EndWhile
        EndTriggerWave()
        Return True
    EndIf
    B21WaveActors = spawned
    ReconcileTriggerWave()
    Return True
EndFunction

; Death events can interleave with this walk, so each actor is located by Find rather than by a stale index.
Function ReconcileTriggerWave()
    Int index = 0
    If B21WaveActors != None
        index = B21WaveActors.Length - 1
    EndIf
    While index >= 0 && B21WaveActors != None
        If index < B21WaveActors.Length
            Actor waveActor = B21WaveActors[index]
            If waveActor == None || waveActor.IsDead()
                Int at = -1
                If B21WaveActors != None
                    at = B21WaveActors.Find(waveActor)
                EndIf
                If at >= 0
                    B21WaveActors.Remove(at)
                    If waveActor != None
                        RemoveTriggerWaveActor(waveActor)
                    EndIf
                EndIf
            EndIf
        EndIf
        index -= 1
    EndWhile
    If B21WaveActors != None && B21WaveActors.Length == 0
        EndTriggerWave()
    EndIf
EndFunction

Keyword Function ResolveTriggerWaveKeyword(B21:EncounterWaveCatalog akCatalog, Actor akPlayer)
    If akCatalog.KeywordVariantCount(EncounterTypeKeyword) > 0
        Return EncounterTypeKeyword
    EndIf
    ; LocEncMain_Current (or no keyword) meant the spawn system's main creature for this location, which Tales rolls.
    If Game.IsPluginInstalled("B21_TalesFromAppalachia.esm")
        Var[] args = New Var[1]
        args[0] = akPlayer
        Keyword rolled = Utility.CallGlobalFunction("B21:B21_TFA_RoamingEventsNative", "CreatureKeyword", args) as Keyword
        If akCatalog.KeywordVariantCount(rolled) > 0
            Return rolled
        EndIf
    EndIf
    Return akCatalog.RandomPoolKeyword()
EndFunction

Int Function TriggerWaveActorCount()
    If BossWave
        If BossWaveAllowLackeys
            Return 1 + Utility.RandomInt(0, 2)
        EndIf
        Return 1
    EndIf
    If Difficulty < 0 || Difficulty > 4
        Return 3
    EndIf
    Return Difficulty + 1
EndFunction

Function RemoveTriggerWaveActor(Actor akActor)
    ; A tracked actor something else deleted reads back as None after a load.
    If akActor == None
        Return
    EndIf
    UnregisterForRemoteEvent(akActor, "OnDeath")
    If !akActor.IsDead()
        akActor.DisableNoWait()
    EndIf
    ; Delete leaves a corpse lootable until its cell unloads.
    akActor.Delete()
EndFunction

Function EndTriggerWave()
    B21WaveActors = None
    B21WaveEndedDay = Utility.GetCurrentGameTime()
EndFunction

; ResetTimeSeconds counts from the end of the wave. A negative value re-arms only when the cell resets,
; which clears this script's variables. Game time stands in for the real-time cooldown so it survives
; the trigger unloading; waiting therefore shortens it.
Bool Function TriggerWaveResetElapsed()
    If B21WaveActors != None || B21WaveEndedDay <= 0.0 || ResetTimeSeconds < 0.0
        Return False
    EndIf
    Float timeScale = 20.0
    GlobalVariable timeScaleGlobal = Game.GetFormFromFile(0x0000003A, "Fallout4.esm") as GlobalVariable
    If timeScaleGlobal != None && timeScaleGlobal.GetValue() > 0.0
        timeScale = timeScaleGlobal.GetValue()
    EndIf
    Return (Utility.GetCurrentGameTime() - B21WaveEndedDay) * 86400.0 / timeScale >= ResetTimeSeconds
EndFunction

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    UnregisterForRemoteEvent(akSender, "OnDeath")
    If B21WaveActors != None && B21WaveActors.Find(akSender) >= 0
        ReconcileTriggerWave()
    EndIf
EndEvent

; Leaving the area ends the wave, as FO76's quest instance expired once its players were gone.
Event OnCellDetach()
    If B21WaveActors == None
        Return
    EndIf
    Actor[] remaining = B21WaveActors
    EndTriggerWave()
    Int index = 0
    While index < remaining.Length
        RemoveTriggerWaveActor(remaining[index])
        index += 1
    EndWhile
EndEvent
