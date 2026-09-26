Event OnAliasInit()
    OwningQuest = GetOwningQuest()
    QS = OwningQuest as Quests:E01C_Tales:Dark:QuestScript
    numGathered = 0
    spawning = False
    KindlingRefs = None
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != 5901 || !spawning
        Return
    EndIf
    RespawnKindling()
    ScheduleRespawn()
EndEvent

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If akSender != Game.GetPlayer() || akBaseItem != E01C_Tales_Dark_DryKindling || OwningQuest == None
        Return
    EndIf
    If akItemReference != None && KindlingRefs != None
        Int slot = KindlingRefs.Find(akItemReference)
        If slot >= 0
            KindlingRefs[slot] = None
        EndIf
    EndIf
    If OwningQuest.IsStageDone(500) && !OwningQuest.IsStageDone(Stage_GatheredKindling)
        numGathered += aiItemCount
        Int required = 10
        If QS != None && QS.KindlingReq > 0
            required = QS.KindlingReq
        EndIf
        If numGathered >= required
            OwningQuest.SetStage(Stage_GatheredKindling)
        EndIf
    EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If akSender == None || akActionRef != playerRef
        Return
    EndIf
    If Alias_Activators_WetKindling == None || Alias_Activators_WetKindling.Find(akSender) < 0
        Return
    EndIf
    UnregisterForRemoteEvent(akSender, "OnActivate")
    Alias_Activators_WetKindling.RemoveRef(akSender)
    If KindlingRefs != None
        Int slot = KindlingRefs.Find(akSender)
        If slot >= 0
            KindlingRefs[slot] = None
        EndIf
    EndIf
    SpawnInsectsAt(akSender, playerRef)
    akSender.DisableNoWait()
    akSender.Delete()
EndEvent

Function StartSpawning()
    If spawning
        Return
    EndIf
    If OwningQuest == None
        OwningQuest = GetOwningQuest()
        QS = OwningQuest as Quests:E01C_Tales:Dark:QuestScript
    EndIf
    spawning = True
    numGathered = 0
    RemoveAllInventoryEventFilters()
    AddInventoryEventFilter(E01C_Tales_Dark_DryKindling)
    RegisterForRemoteEvent(Game.GetPlayer(), "OnItemAdded")
    RespawnKindling()
    ScheduleRespawn()
EndFunction

Function StopSpawning()
    spawning = False
    CancelTimer(5901)
EndFunction

Function ScheduleRespawn()
    Float delay = 20.0
    If QS != None && QS.RefreshRate > 0.0
        delay = QS.RefreshRate
    EndIf
    StartTimer(delay, 5901)
EndFunction

; Each kindling marker keeps one pickup; an emptied marker is restocked on the refresh tick so the fire can be fed until the event ends.
Function RespawnKindling()
    If !spawning || OwningQuest == None || !OwningQuest.IsRunning()
        Return
    EndIf
    Int markerCount = GetCount()
    If markerCount <= 0
        Return
    EndIf
    If KindlingRefs == None || KindlingRefs.Length != markerCount
        KindlingRefs = New ObjectReference[markerCount]
    EndIf
    Int index = 0
    While index < markerCount
        ObjectReference current = KindlingRefs[index]
        If current == None || current.GetContainer() != None || current.IsDisabled() || current.IsDeleted()
            ObjectReference marker = GetAt(index)
            If marker != None
                KindlingRefs[index] = SpawnKindlingAt(marker)
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

ObjectReference Function SpawnKindlingAt(ObjectReference akMarker)
    Float wetChance = 0.0
    If QS != None
        wetChance = QS.ChanceToSpawnWet
    EndIf
    If E01C_Tales_Dark_WetTwigsActivator01 != None && Utility.RandomFloat(0.0, 1.0) < wetChance
        ObjectReference wetKindling = akMarker.PlaceAtMe(E01C_Tales_Dark_WetTwigsActivator01, 1, True)
        If wetKindling != None
            If Alias_Activators_WetKindling != None
                Alias_Activators_WetKindling.AddRef(wetKindling)
            EndIf
            RegisterForRemoteEvent(wetKindling, "OnActivate")
        EndIf
        Return wetKindling
    EndIf
    If E01C_Tales_Dark_DryKindling == None
        Return None
    EndIf
    ObjectReference dryKindling = akMarker.PlaceAtMe(E01C_Tales_Dark_DryKindling, 1, True)
    If dryKindling != None && Alias_QO_Misc_DryKindling != None
        Alias_QO_Misc_DryKindling.AddRef(dryKindling)
    EndIf
    Return dryKindling
EndFunction

Function SpawnInsectsAt(ObjectReference akSource, Actor akTarget)
    If QS == None || akSource == None
        Return
    EndIf
    ActorBase[] insectBases = QS.InsectsToSpawn
    If insectBases == None || insectBases.Length == 0
        Return
    EndIf
    Int index = 0
    While index < QS.NumInsectsToSpawn
        ActorBase insectBase = insectBases[Utility.RandomInt(0, insectBases.Length - 1)]
        If insectBase != None
            Actor insect = akSource.PlaceActorAtMe(insectBase)
            If insect != None
                If Actors_Insects != None
                    Actors_Insects.AddRef(insect)
                EndIf
                If akTarget != None
                    insect.StartCombat(akTarget)
                EndIf
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Function CleanupKindling()
    StopSpawning()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnItemAdded")
    EndIf
    RemoveAllInventoryEventFilters()
    Int index = 0
    While KindlingRefs != None && index < KindlingRefs.Length
        ObjectReference kindling = KindlingRefs[index]
        If kindling != None
            UnregisterForRemoteEvent(kindling, "OnActivate")
            If kindling.GetContainer() == None
                kindling.DisableNoWait()
                kindling.Delete()
            EndIf
        EndIf
        KindlingRefs[index] = None
        index += 1
    EndWhile
    If Actors_Insects != None
        index = Actors_Insects.GetCount() - 1
        While index >= 0
            ObjectReference insect = Actors_Insects.GetAt(index)
            If insect != None
                Actors_Insects.RemoveRef(insect)
                insect.DisableNoWait()
                insect.Delete()
            EndIf
            index -= 1
        EndWhile
    EndIf
EndFunction
