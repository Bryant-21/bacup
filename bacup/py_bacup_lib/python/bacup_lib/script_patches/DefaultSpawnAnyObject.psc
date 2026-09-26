Event OnQuestInit()
    blockedLocations = new ObjectReference[0]
    CurrentObjectsToSpawn = new ObjectProperties[0]
    currentStageIndex = -1
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    HandleStage(auiStageID)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID >= 7650 && aiTimerID < 7650 + 128 && IsRunning()
        SpawnObjectEntry(aiTimerID - 7650, False)
    EndIf
EndEvent

Event OnQuestShutdown()
    CleanUpObjects(-1)
    blockedLocations = new ObjectReference[0]
EndEvent

Function HandleStage(Int aiStageID)
    If StagesProperties == None
        Return
    EndIf
    Int index = 0
    While index < StagesProperties.Length
        StagesStruct stage = StagesProperties[index]
        If stage != None && stage.cleanUpStage >= 0 && stage.cleanUpStage == aiStageID
            If stage.bFullCleanUp
                CleanUpObjects(-1)
                blockedLocations = new ObjectReference[0]
            Else
                CleanUpObjects(index)
            EndIf
        EndIf
        index += 1
    EndWhile
    index = 0
    While index < StagesProperties.Length
        StagesStruct stage = StagesProperties[index]
        If stage != None && stage.preReqStage == aiStageID
            currentStageIndex = index
            SpawnForStage(index)
        EndIf
        index += 1
    EndWhile
EndFunction

Function SpawnForStage(Int aiStageIndex)
    Int index = 0
    While ObjectsToSpawn != None && index < ObjectsToSpawn.Length && index < 128
        ObjectProperties entry = ObjectsToSpawn[index]
        If entry != None && entry.StageIndex == aiStageIndex && entry.Object != None
            If CurrentObjectsToSpawn.Find(entry) < 0
                CurrentObjectsToSpawn.Add(entry)
            EndIf
            If entry.SpawnDelay > 0.0 && entry.bDelayVFX
                StartTimer(entry.SpawnDelay, 7650 + index)
            Else
                SpawnObjectEntry(index, True)
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

; Locations come from the single alias or the location collection; each is used once per entry.
Function SpawnObjectEntry(Int aiEntryIndex, Bool abAllowDelay)
    ObjectProperties entry = ObjectsToSpawn[aiEntryIndex]
    If entry == None || entry.Object == None
        Return
    EndIf
    StagesStruct stage = None
    If entry.StageIndex >= 0 && entry.StageIndex < StagesProperties.Length
        stage = StagesProperties[entry.StageIndex]
    EndIf
    ObjectReference[] locations = ChooseLocations(entry, stage)
    If entry.ChosenSpawnLocations != None
        entry.ChosenSpawnLocations.RemoveAll()
    EndIf
    Int index = 0
    While index < locations.Length
        ObjectReference spawnPoint = locations[index]
        If entry.ChosenSpawnLocations != None
            entry.ChosenSpawnLocations.AddRef(spawnPoint)
        EndIf
        If stage != None && stage.bIgnoreUsedLocations && blockedLocations.Find(spawnPoint) < 0
            blockedLocations.Add(spawnPoint)
        EndIf
        If entry.SpawnEffect != None
            spawnPoint.PlaceAtMe(entry.SpawnEffect)
        EndIf
        index += 1
    EndWhile
    If abAllowDelay && entry.SpawnDelay > 0.0
        Utility.Wait(entry.SpawnDelay)
    EndIf
    index = 0
    While index < locations.Length && IsRunning()
        ObjectReference spawned = locations[index].PlaceAtMe(entry.Object)
        If spawned != None && entry.SpawnedObjects != None
            entry.SpawnedObjects.AddRef(spawned)
        EndIf
        index += 1
    EndWhile
EndFunction

ObjectReference[] Function ChooseLocations(ObjectProperties akEntry, StagesStruct akStage)
    ObjectReference[] chosen = new ObjectReference[0]
    If akEntry.SpawnLocationAlias != None && akEntry.SpawnLocationAlias.GetReference() != None
        Int amount = akEntry.SpawnAmount
        If amount < 1
            amount = 1
        EndIf
        While chosen.Length < amount && chosen.Length < 128
            chosen.Add(akEntry.SpawnLocationAlias.GetReference())
        EndWhile
        Return chosen
    EndIf
    If akEntry.SpawnLocCollection == None
        Return chosen
    EndIf
    Bool skipBlocked = akStage != None && akStage.bIgnoreUsedLocations
    Int groupSize = akEntry.GroupSize
    If groupSize < 1
        groupSize = 1
    EndIf
    ObjectReference[] candidates = new ObjectReference[0]
    Int total = akEntry.SpawnLocCollection.GetCount()
    Int groupStart = 0
    ; With GroupSize > 1 the collection is ordered by adjacency and one location is taken per group.
    While groupStart < total && candidates.Length < 128
        ObjectReference[] groupRefs = new ObjectReference[0]
        Int index = groupStart
        While index < groupStart + groupSize && index < total
            ObjectReference spawnPoint = akEntry.SpawnLocCollection.GetAt(index)
            If spawnPoint != None && !(skipBlocked && blockedLocations.Find(spawnPoint) >= 0)
                groupRefs.Add(spawnPoint)
            EndIf
            index += 1
        EndWhile
        If groupRefs.Length > 0
            If groupSize > 1 || akEntry.bRandomSpawns
                candidates.Add(groupRefs[Utility.RandomInt(0, groupRefs.Length - 1)])
            Else
                candidates.Add(groupRefs[0])
            EndIf
        EndIf
        groupStart += groupSize
    EndWhile
    Int wanted = akEntry.SpawnAmount
    If wanted < 0 || wanted > candidates.Length
        wanted = candidates.Length
    EndIf
    While chosen.Length < wanted
        Int pick = 0
        If akEntry.bRandomSpawns
            pick = Utility.RandomInt(0, candidates.Length - 1)
        EndIf
        chosen.Add(candidates[pick])
        candidates.Remove(pick)
    EndWhile
    Return chosen
EndFunction

Function CleanUpObjects(Int aiStageIndex)
    Int index = 0
    While ObjectsToSpawn != None && index < ObjectsToSpawn.Length
        ObjectProperties entry = ObjectsToSpawn[index]
        If entry != None && (aiStageIndex < 0 || entry.StageIndex == aiStageIndex)
            If entry.SpawnedObjects != None
                Int member = entry.SpawnedObjects.GetCount() - 1
                While member >= 0
                    ObjectReference spawned = entry.SpawnedObjects.GetAt(member)
                    If spawned != None
                        entry.SpawnedObjects.RemoveRef(spawned)
                        ; Picked-up items belong to their container now; only world copies are deleted.
                        If spawned.GetContainer() == None
                            spawned.Disable()
                            spawned.Delete()
                        EndIf
                    EndIf
                    member -= 1
                EndWhile
                entry.SpawnedObjects.RemoveAll()
            EndIf
            If entry.ChosenSpawnLocations != None
                entry.ChosenSpawnLocations.RemoveAll()
            EndIf
            Int slot = CurrentObjectsToSpawn.Find(entry)
            If slot >= 0
                CurrentObjectsToSpawn.Remove(slot)
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction
