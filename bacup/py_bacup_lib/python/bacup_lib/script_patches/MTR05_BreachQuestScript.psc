DefaultQuestEncounterWaveScript Function WaveScript()
    Quest owner = Self as Quest
    Return owner as DefaultQuestEncounterWaveScript
EndFunction

B21:QuestVariables Function QuestVars()
    Quest owner = Self as Quest
    Return owner as B21:QuestVariables
EndFunction

Function SetReferenceEnabled(ObjectReference akReference, Bool abEnabled)
    If akReference == None
        Return
    EndIf
    If abEnabled
        akReference.Enable(False)
    Else
        akReference.Disable(False)
    EndIf
EndFunction

Function SetAliasEnabled(ReferenceAlias akAlias, Bool abEnabled)
    If akAlias != None
        SetReferenceEnabled(akAlias.GetReference(), abEnabled)
    EndIf
EndFunction

Function SetCollectionEnabled(RefCollectionAlias akCollection, Bool abEnabled)
    If akCollection == None
        Return
    EndIf
    Int index = 0
    While index < akCollection.GetCount()
        SetReferenceEnabled(akCollection.GetAt(index), abEnabled)
        index += 1
    EndWhile
EndFunction

Function SetSpawnMarkersEnabled(Bool abEnabled)
    If EnemySpawns == None
        Return
    EndIf
    Int index = 0
    While index < EnemySpawns.Length
        SetAliasEnabled(EnemySpawns[index], abEnabled)
        index += 1
    EndWhile
EndFunction

Function PlaySceneIfIdle(Scene akScene)
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
EndFunction

Function StopSceneIfPlaying(Scene akScene)
    If akScene != None && akScene.IsPlaying()
        akScene.Stop()
    EndIf
EndFunction

Function PublishContainerCounts()
    B21:QuestVariables questVars = QuestVars()
    If questVars == None
        Return
    EndIf
    Int total = 0
    If Containers != None
        total = Containers.GetCount()
    EndIf
    Int unsealed = 0
    If UnlockedContainers != None
        unsealed = UnlockedContainers.GetCount()
    EndIf
    questVars.SetVariable(TotalContainersString, total as Float)
    questVars.SetVariable(ContainersUnlockedString, unsealed as Float)
EndFunction

Function AnnounceClearEnemies()
    ; The site loudspeaker carries the Motherlode's lines. Both are site references, so a
    ; location that lost either one simply stays quiet.
    If MTR05_ClearEnemies == None || Loudspeaker == None
        Return
    EndIf
    ObjectReference speakerRef = Loudspeaker.GetReference()
    If speakerRef == None
        Return
    EndIf
    Actor voiceActor = None
    If MotherlodeVoice != None
        voiceActor = MotherlodeVoice.GetActorReference()
    EndIf
    speakerRef.Say(MTR05_ClearEnemies, voiceActor)
EndFunction

Function StartBreachWaves()
    DefaultQuestEncounterWaveScript waveScript = WaveScript()
    If waveScript == None
        Return
    EndIf
    ; Waves three and four ship as OBSOLETE in the record, so only the two live waves run.
    waveScript.StartEncounterWaveByID("Wave One")
    waveScript.StartEncounterWaveByID("Wave Two")
EndFunction

Function StartFinalWave()
    DefaultQuestEncounterWaveScript waveScript = WaveScript()
    If waveScript != None
        waveScript.StartEncounterWaveByID("Endless Wave")
    EndIf
EndFunction

Function StopBreachWaves(Bool abRemoveActors)
    DefaultQuestEncounterWaveScript waveScript = WaveScript()
    If waveScript != None
        waveScript.StopAllEncounterWaves(abRemoveActors)
    EndIf
EndFunction

Function CancelBreachTimers()
    CancelTimer(iBreachExplosionID)
    CancelTimer(101)
    CancelTimer(102)
    CancelTimer(103)
    CancelTimer(104)
    CancelTimer(105)
EndFunction

Function StartBreachEvent()
    CancelBreachTimers()
    bFinalBatch = False
    iLockSoundCount = 0
    If UnlockedContainers != None
        UnlockedContainers.RemoveAll()
    EndIf
    If PreppedContainers != None
        PreppedContainers.RemoveAll()
    EndIf
    SetAliasEnabled(EnableMarker, True)
    SetCollectionEnabled(Klaxons, True)
    SetSpawnMarkersEnabled(True)
    If MapMarker != None && MapMarker.GetReference() != None
        MapMarker.GetReference().Enable(False)
        MapMarker.GetReference().AddToMap(False)
    EndIf
    PublishContainerCounts()
    StartBreachWaves()
    AnnounceClearEnemies()
EndFunction

Function BeginBreach()
    ; FO76 drove the surfacing from a scene. The bound timer lengths keep the chain moving
    ; when the converted scene cannot play.
    CancelTimer(iBreachExplosionID)
    StartTimer(iBreachTimerLength, iBreachExplosionID)
EndFunction

Function DetonateBreach()
    ObjectReference explosionRef = None
    If BreachExplosionMarker != None
        explosionRef = BreachExplosionMarker.GetReference()
    EndIf
    If explosionRef != None && MTR05_MotherlodeBreachExplosion != None
        explosionRef.PlaceAtMe(MTR05_MotherlodeBreachExplosion)
    EndIf
    Actor playerRef = Game.GetPlayer()
    If MTR05_CameraShakeSpell != None && explosionRef != None && playerRef != None
        MTR05_CameraShakeSpell.Cast(explosionRef, playerRef)
    EndIf
    SetAliasEnabled(Motherlode, True)
    PlaySceneIfIdle(MTR05_Mother_Breach_0050_Scene)
    CancelTimer(101)
    StartTimer(iAnimTimerLength, 101)
EndFunction

Float Function ContainerFillChance()
    If MTR05_Breach_ChanceStandardContainerFilled != None && MTR05_Breach_ChanceStandardContainerFilled.GetValue() > 0.0
        Return MTR05_Breach_ChanceStandardContainerFilled.GetValue()
    EndIf
    Return 1.0
EndFunction

Float Function RareItemChance()
    If MTR05_Breach_RareItemPercentChance != None
        Return MTR05_Breach_RareItemPercentChance.GetValue()
    EndIf
    Return 0.0
EndFunction

Function FillContainer(ObjectReference akContainerRef, Bool abRare)
    If akContainerRef == None
        Return
    EndIf
    If Utility.RandomFloat(0.0, 1.0) > ContainerFillChance()
        Return
    EndIf
    If abRare
        If LLI_MTR05_MotherlodeContainer_Rare != None
            akContainerRef.AddItem(LLI_MTR05_MotherlodeContainer_Rare, 1, True)
        EndIf
        Return
    EndIf
    If LLI_MTR05_MotherlodeContainer != None
        akContainerRef.AddItem(LLI_MTR05_MotherlodeContainer, 1, True)
    EndIf
    If LLI_MTR05_MotherlodeContainer_Rare != None && Utility.RandomFloat(0.0, 1.0) <= RareItemChance()
        akContainerRef.AddItem(LLI_MTR05_MotherlodeContainer_Rare, 1, True)
    EndIf
EndFunction

Function UnsealContainer(ObjectReference akContainerRef, Bool abRare)
    If akContainerRef == None
        Return
    EndIf
    akContainerRef.Enable(False)
    FillContainer(akContainerRef, abRare)
    ; Green means open to everyone; yellow is the Hornwright-ID crate FO76 kept locked.
    String animationState = OpenState
    Sound cue = DRScMTR05MotherlodeContainerUnlock
    If abRare
        animationState = LockedState
        cue = DRScMTR05MotherlodeContainerLock
    EndIf
    If animationState != ""
        akContainerRef.PlayAnimation(animationState)
    EndIf
    ; iLockSoundMax keeps a whole batch of crates from firing the cue at once.
    If cue != None && iLockSoundCount < iLockSoundMax
        cue.Play(akContainerRef)
        iLockSoundCount += 1
    EndIf
    If !abRare && UnlockedContainers != None && UnlockedContainers.Find(akContainerRef) < 0
        UnlockedContainers.AddRef(akContainerRef)
    EndIf
EndFunction

Function PrepareContainers()
    If Containers == None || PreppedContainers == None
        Return
    EndIf
    PreppedContainers.RemoveAll()
    Int index = 0
    While index < Containers.GetCount()
        ObjectReference containerRef = Containers.GetAt(index)
        If containerRef != None
            containerRef.Enable(False)
            PreppedContainers.AddRef(containerRef)
        EndIf
        index += 1
    EndWhile

    Float batchPercent = 0.0
    If MTR05_Breach_PercentContainersOpeningPerBatch != None
        batchPercent = MTR05_Breach_PercentContainersOpeningPerBatch.GetValue()
    EndIf
    iContainersPerBatch = Math.Ceiling(PreppedContainers.GetCount() * batchPercent)
    If iContainersPerBatch < 1
        iContainersPerBatch = 1
    EndIf
EndFunction

Function PrepareRareContainers()
    If RareContainers == None
        Return
    EndIf
    iLockSoundCount = 0
    Int index = 0
    While index < RareContainers.GetCount()
        UnsealContainer(RareContainers.GetAt(index), True)
        index += 1
    EndWhile
EndFunction

Function SpawnRareWeapon()
    If RareWeaponSpawn == None || LL_MTR05_RareWeapon == None
        Return
    EndIf
    ObjectReference spawnRef = RareWeaponSpawn.GetReference()
    If spawnRef == None || Utility.RandomInt(1, 100) > iRareWeaponPercent
        Return
    EndIf
    spawnRef.PlaceAtMe(LL_MTR05_RareWeapon)
EndFunction

Function BeginContainerCollection()
    PrepareContainers()
    PrepareRareContainers()
    SpawnRareWeapon()
    StartFinalWave()
    UnsealNextBatch()
EndFunction

Function UnsealNextBatch()
    iLockSoundCount = 0
    Int unsealed = 0
    While unsealed < iContainersPerBatch && PreppedContainers != None && PreppedContainers.GetCount() > 0
        ObjectReference containerRef = PreppedContainers.GetAt(0)
        PreppedContainers.RemoveRef(containerRef)
        UnsealContainer(containerRef, False)
        unsealed += 1
    EndWhile
    PublishContainerCounts()
    PlaySceneIfIdle(MTR05_Mother_Breach_0060_NewContainersUnlocked)

    If PreppedContainers == None || PreppedContainers.GetCount() == 0
        bFinalBatch = True
        SetObjectiveDisplayed(iAdditionalContainerObjIndex, False)
        CancelTimer(102)
        If !GetStageDone(70)
            SetStage(70)
        EndIf
        Return
    EndIf
    SetObjectiveDisplayed(iAdditionalContainerObjIndex, True, True)
    CancelTimer(102)
    StartTimer(iOpenBatchLength as Float, 102)
EndFunction

Function StopContainerCollection()
    CancelTimer(102)
    SetObjectiveDisplayed(iAdditionalContainerObjIndex, False)
EndFunction

Function ScheduleCompletion()
    ; The machine submerges before the quest wraps up.
    CancelTimer(105)
    StartTimer(iAnimTimerLength, 105)
EndFunction

Function ScheduleShutdown(Float afSeconds)
    CancelTimer(103)
    StartTimer(afSeconds, 103)
EndFunction

Function ScheduleFailureShutdown(Float afSeconds)
    CancelTimer(104)
    StartTimer(afSeconds, 104)
EndFunction

Function CleanupBreach(Bool abRemoveActors)
    CancelBreachTimers()
    StopBreachWaves(abRemoveActors)
    StopSceneIfPlaying(MTR05_Mother_Breach_0045_BreachWarning)
    StopSceneIfPlaying(MTR05_Mother_Breach_0050_Scene)
    StopSceneIfPlaying(MTR05_Mother_Breach_0060_NewContainersUnlocked)
    StopSceneIfPlaying(MTR05_Mother_Breach_0070_InitiatingSubmerge)
    StopSceneIfPlaying(MTR05_Mother_Breach_0100_Submerge)
    SetCollectionEnabled(Klaxons, False)
    SetCollectionEnabled(Containers, False)
    SetCollectionEnabled(RareContainers, False)
    SetAliasEnabled(Motherlode, False)
    SetSpawnMarkersEnabled(False)
    SetAliasEnabled(EnableMarker, False)
    If UnlockedContainers != None
        UnlockedContainers.RemoveAll()
    EndIf
    If PreppedContainers != None
        PreppedContainers.RemoveAll()
    EndIf
    If HeardIntroColl != None
        HeardIntroColl.RemoveAll()
    EndIf
    bFinalBatch = False
    iLockSoundCount = 0
    PublishContainerCounts()
EndFunction

Event OnQuestShutdown()
    CancelBreachTimers()
EndEvent

Event OnTimer(Int aiTimerID)
    If !IsRunning()
        Return
    EndIf
    If aiTimerID == iBreachExplosionID
        DetonateBreach()
    ElseIf aiTimerID == 101
        If !GetStageDone(iCollectStage)
            SetStage(iCollectStage)
        EndIf
    ElseIf aiTimerID == 105
        If !GetStageDone(iShutdownStage)
            SetStage(iShutdownStage)
        EndIf
    ElseIf aiTimerID == 102
        If GetStageDone(70) || GetStageDone(100) || GetStageDone(iShutdownStage) || GetStageDone(300)
            Return
        EndIf
        UnsealNextBatch()
    ElseIf aiTimerID == 103
        If !IsStopping() && !IsStopped()
            Stop()
        EndIf
    ElseIf aiTimerID == 104
        If !GetStageDone(305)
            SetStage(305)
        EndIf
    EndIf
EndEvent
