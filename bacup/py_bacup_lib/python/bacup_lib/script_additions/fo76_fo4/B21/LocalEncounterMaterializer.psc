Scriptname B21:LocalEncounterMaterializer Extends Quest
{Materializes the fixed local actor pools used by converted FO76 quest waves.

FO76 fills these aliases through its server-side Encounter Wave System. The
converter binds only quests whose source WAVE records and local wave contract
can be represented exactly in FO4. Actors are created disabled so the existing
DefaultQuestEncounterWaveScript remains responsible for enabling them, combat,
death tracking, and wave-end stages.}

RefCollectionAlias[] Property WaveCollections Auto Const
ReferenceAlias[] Property WaveSpawnMarkers Auto Const
Int[] Property WavePreparationStages Auto Const
Int[] Property StageAfterPreparation Auto Const
{Exact follow-up stage for a shared-alias replacement whose original fragment checks that alias in parallel.}
Bool[] Property ReplaceCollectionOnPrepare Auto Const
Int[] Property WaveActorCounts Auto Const
Int[] Property WaveSpawnSlotCounts Auto Const
Form[] Property CandidateForms Auto Const
Int[] Property CandidateWaveIndices Auto Const
Int[] Property CandidateSpawnSlots Auto Const

Int[] PreparedWaveIndices
ObjectReference[] SpawnedReferences
RefCollectionAlias[] SpawnedCollections
Bool ShuttingDown

Event OnQuestInit()
    ShuttingDown = False
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
    PrepareEligibleWaves()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    PrepareEligibleWaves()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer() && !ShuttingDown
        PrepareEligibleWaves()
    EndIf
EndEvent

Event OnQuestShutdown()
    ShuttingDown = True
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
    ClearAllMaterializedReferences()
EndEvent

Function PrepareEligibleWaves()
    If ShuttingDown || !WaveContractIsValid()
        Return
    EndIf

    Int waveIndex = 0
    While waveIndex < WaveCollections.Length
        Int preparationStage = WavePreparationStages[waveIndex]
        If !HasPreparedWave(waveIndex) && (preparationStage < 0 || IsStageDone(preparationStage))
            PrepareWave(waveIndex)
        EndIf
        waveIndex += 1
    EndWhile
EndFunction

Function PrepareWave(Int aiWaveIndex)
    RefCollectionAlias waveCollection = WaveCollections[aiWaveIndex]
    ReferenceAlias spawnMarkerAlias = WaveSpawnMarkers[aiWaveIndex]
    If waveCollection == None || spawnMarkerAlias == None
        Return
    EndIf

    If ReplaceCollectionOnPrepare[aiWaveIndex]
        ClearMaterializedCollection(waveCollection)
    ElseIf waveCollection.GetCount() > 0
        RecordPreparedWave(aiWaveIndex)
        Return
    EndIf

    ObjectReference spawnMarker = spawnMarkerAlias.GetReference()
    Int actorCount = WaveActorCounts[aiWaveIndex]
    If spawnMarker == None || actorCount <= 0
        Return
    EndIf

    Int actorIndex = 0
    While actorIndex < actorCount
        Form actorBase = SelectCandidateForm(aiWaveIndex, actorIndex)
        ObjectReference spawnedRef
        If actorBase != None
            spawnedRef = spawnMarker.PlaceAtMe(actorBase, 1, True, True, False)
        EndIf
        If spawnedRef == None
            ClearMaterializedCollection(waveCollection)
            Return
        EndIf

        waveCollection.AddRef(spawnedRef)
        TrackMaterializedReference(spawnedRef, waveCollection)
        actorIndex += 1
    EndWhile

    RecordPreparedWave(aiWaveIndex)
    Int nextStage = StageAfterPreparation[aiWaveIndex]
    If nextStage >= 0 && !IsStageDone(nextStage)
        SetStage(nextStage)
    EndIf
EndFunction

Form Function SelectCandidateForm(Int aiWaveIndex, Int aiActorIndex)
    Int slotCount = WaveSpawnSlotCounts[aiWaveIndex]
    If slotCount <= 0
        Return None
    EndIf
    Int spawnSlot = aiActorIndex % slotCount
    Int candidateCount = 0
    Int candidateIndex = 0
    While candidateIndex < CandidateForms.Length
        If CandidateWaveIndices[candidateIndex] == aiWaveIndex && CandidateSpawnSlots[candidateIndex] == spawnSlot && CandidateForms[candidateIndex] != None
            candidateCount += 1
        EndIf
        candidateIndex += 1
    EndWhile
    If candidateCount <= 0
        Return None
    EndIf

    Int selectedCandidate = Utility.RandomInt(0, candidateCount - 1)
    candidateIndex = 0
    While candidateIndex < CandidateForms.Length
        If CandidateWaveIndices[candidateIndex] == aiWaveIndex && CandidateSpawnSlots[candidateIndex] == spawnSlot && CandidateForms[candidateIndex] != None
            If selectedCandidate == 0
                Return CandidateForms[candidateIndex]
            EndIf
            selectedCandidate -= 1
        EndIf
        candidateIndex += 1
    EndWhile
    Return None
EndFunction

Bool Function WaveContractIsValid()
    If WaveCollections == None || WaveSpawnMarkers == None || WavePreparationStages == None || StageAfterPreparation == None || ReplaceCollectionOnPrepare == None || WaveActorCounts == None || WaveSpawnSlotCounts == None
        Return False
    EndIf
    If CandidateForms == None || CandidateWaveIndices == None || CandidateSpawnSlots == None
        Return False
    EndIf
    If WaveCollections.Length != WaveSpawnMarkers.Length || WaveCollections.Length != WavePreparationStages.Length || WaveCollections.Length != StageAfterPreparation.Length || WaveCollections.Length != ReplaceCollectionOnPrepare.Length || WaveCollections.Length != WaveActorCounts.Length || WaveCollections.Length != WaveSpawnSlotCounts.Length
        Return False
    EndIf
    Return CandidateForms.Length == CandidateWaveIndices.Length && CandidateForms.Length == CandidateSpawnSlots.Length
EndFunction

Bool Function HasPreparedWave(Int aiWaveIndex)
    Return PreparedWaveIndices != None && PreparedWaveIndices.Find(aiWaveIndex) >= 0
EndFunction

Function RecordPreparedWave(Int aiWaveIndex)
    If HasPreparedWave(aiWaveIndex)
        Return
    EndIf
    If PreparedWaveIndices == None
        PreparedWaveIndices = New Int[1]
        PreparedWaveIndices[0] = aiWaveIndex
    Else
        PreparedWaveIndices.Add(aiWaveIndex)
    EndIf
EndFunction

Function TrackMaterializedReference(ObjectReference akReference, RefCollectionAlias akCollection)
    If SpawnedReferences == None
        SpawnedReferences = New ObjectReference[1]
        SpawnedCollections = New RefCollectionAlias[1]
        SpawnedReferences[0] = akReference
        SpawnedCollections[0] = akCollection
    Else
        SpawnedReferences.Add(akReference)
        SpawnedCollections.Add(akCollection)
    EndIf
EndFunction

Function ClearMaterializedCollection(RefCollectionAlias akCollection)
    If akCollection == None || SpawnedReferences == None || SpawnedCollections == None
        Return
    EndIf

    Int referenceIndex = 0
    While referenceIndex < SpawnedReferences.Length && referenceIndex < SpawnedCollections.Length
        ObjectReference spawnedRef = SpawnedReferences[referenceIndex]
        If spawnedRef != None && SpawnedCollections[referenceIndex] == akCollection
            akCollection.RemoveRef(spawnedRef)
            spawnedRef.DisableNoWait()
            spawnedRef.Delete()
            SpawnedReferences[referenceIndex] = None
            SpawnedCollections[referenceIndex] = None
        EndIf
        referenceIndex += 1
    EndWhile
EndFunction

Function ClearAllMaterializedReferences()
    If SpawnedReferences == None || SpawnedCollections == None
        Return
    EndIf

    Int referenceIndex = 0
    While referenceIndex < SpawnedReferences.Length && referenceIndex < SpawnedCollections.Length
        ObjectReference spawnedRef = SpawnedReferences[referenceIndex]
        RefCollectionAlias waveCollection = SpawnedCollections[referenceIndex]
        If spawnedRef != None
            If waveCollection != None
                waveCollection.RemoveRef(spawnedRef)
            EndIf
            spawnedRef.DisableNoWait()
            spawnedRef.Delete()
            SpawnedReferences[referenceIndex] = None
            SpawnedCollections[referenceIndex] = None
        EndIf
        referenceIndex += 1
    EndWhile
EndFunction
