Int Function SpawnStrikeTimerID()
    ; Offset the FO76 timer ids (0 and 1) away from ids other scripts on this quest may use.
    Return 73300 + TimerID_SpawnStrikes
EndFunction

Function ResetStrikeState()
    StopStrikes = True
    StopSpawnStrikes = True
    CancelTimer(SpawnStrikeTimerID())
    SetHazardsEnabled(ElectricalHazardsCharged, ElectricalHazardsChargedRefColAlias, False)
    SetHazardsEnabled(ElectricalHazardsBoss, ElectricalHazardsBossRefColAlias, False)
    ShowBossMesh()
EndFunction

Function SetHazardsEnabled(ObjectReference[] akHazards, RefCollectionAlias akCollection, Bool abEnabled)
    Int index = 0
    While akHazards != None && index < akHazards.Length
        ObjectReference hazardRef = akHazards[index]
        If hazardRef != None
            If abEnabled
                hazardRef.Enable()
                If akCollection != None && akCollection.Find(hazardRef) < 0
                    akCollection.AddRef(hazardRef)
                EndIf
            Else
                hazardRef.Disable()
            EndIf
        EndIf
        index += 1
    EndWhile
    If !abEnabled && akCollection != None
        akCollection.RemoveAll()
    EndIf
EndFunction

Function ShowBossMesh()
    If Storm_E01_BossMesh != None && Storm_E01_BossMesh.GetReference() != None
        Storm_E01_BossMesh.GetReference().Enable()
    EndIf
EndFunction

Int Function CountLivingChargedMobs()
    Int living = 0
    Int index = 0
    While ChargedMobs != None && index < ChargedMobs.GetCount()
        Actor charged = ChargedMobs.GetAt(index) as Actor
        If charged != None && !charged.IsDead()
            living += 1
        EndIf
        index += 1
    EndWhile
    Return living
EndFunction

Function ScheduleSpawnStrikes()
    Int minSeconds = SpawnTimeBetweenStrikesMin
    If minSeconds < 1
        minSeconds = 1
    EndIf
    Int maxSeconds = SpawnTimeBetweenStrikesMax
    If maxSeconds < minSeconds
        maxSeconds = minSeconds
    EndIf
    StartTimer(Utility.RandomInt(minSeconds, maxSeconds) as Float, SpawnStrikeTimerID())
EndFunction

Function StrikeSpawnMarkers()
    If SpawnStrikeMarkers == None || SpawnStrikeMarkers.Length == 0 || Storm_E01_LCharChargedEnemies == None || ChargedMobs == None
        Return
    EndIf
    Int markerCount = SpawnStrikeMarkers.Length
    ; Storm_E01_ChargedStrikeChance capped FO76 spawns for performance; FO4 keeps at most one living charged enemy per marker.
    Int living = CountLivingChargedMobs()
    Int offset = Utility.RandomInt(0, markerCount - 1)
    Int index = 0
    While index < markerCount && living < markerCount
        ObjectReference marker = SpawnStrikeMarkers[(offset + index) % markerCount]
        If marker != None
            Actor charged = marker.PlaceAtMe(Storm_E01_LCharChargedEnemies) as Actor
            If charged != None
                ChargedMobs.AddRef(charged)
                living += 1
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Function StartChargedStrikes()
    If !IsRunning()
        Return
    EndIf
    StopStrikes = False
    StopSpawnStrikes = False
    SetHazardsEnabled(ElectricalHazardsCharged, ElectricalHazardsChargedRefColAlias, True)
    CancelTimer(SpawnStrikeTimerID())
    ScheduleSpawnStrikes()
EndFunction

Function StopChargedStrikes()
    StopSpawnStrikes = True
    CancelTimer(SpawnStrikeTimerID())
    SetHazardsEnabled(ElectricalHazardsCharged, ElectricalHazardsChargedRefColAlias, False)
EndFunction

Function ActivateBoss()
    If !IsRunning()
        Return
    EndIf
    SetHazardsEnabled(ElectricalHazardsBoss, ElectricalHazardsBossRefColAlias, True)
    If Storm_E01_BossMesh != None && Storm_E01_BossMesh.GetReference() != None
        Storm_E01_BossMesh.GetReference().Disable()
    EndIf
    If Storm_E01_Boss != None
        Actor boss = Storm_E01_Boss.GetActorReference()
        If boss != None
            boss.Enable()
            boss.EvaluatePackage()
        EndIf
    EndIf
EndFunction

Function KillChargedMobs()
    If ChargedMobs == None
        Return
    EndIf
    Int index = ChargedMobs.GetCount() - 1
    While index >= 0
        Actor charged = ChargedMobs.GetAt(index) as Actor
        If charged != None
            If !charged.IsDead()
                charged.KillSilent()
            EndIf
            charged.Delete()
        EndIf
        index -= 1
    EndWhile
    ChargedMobs.RemoveAll()
EndFunction

Function StopAllStrikes()
    StopStrikes = True
    StopChargedStrikes()
    SetHazardsEnabled(ElectricalHazardsBoss, ElectricalHazardsBossRefColAlias, False)
    KillChargedMobs()
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != SpawnStrikeTimerID()
        Return
    EndIf
    If StopStrikes || StopSpawnStrikes || !IsRunning()
        Return
    EndIf
    StrikeSpawnMarkers()
    ScheduleSpawnStrikes()
EndEvent

Event OnQuestShutdown()
    StopAllStrikes()
    ShowBossMesh()
EndEvent
