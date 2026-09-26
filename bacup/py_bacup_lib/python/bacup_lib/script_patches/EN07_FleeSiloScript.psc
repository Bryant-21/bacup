Bool Function BeginLocalLaunch(Int aiSiloID, Int aiLaunchID, Location akSiloLocation)
    If aiSiloID < 0 || aiSiloID > 2 || aiLaunchID != aiSiloID + 3 || akSiloLocation == None
        Return False
    EndIf
    If IsRunning() && !IsStageDone(300) && iSiloIndex >= 0
        Return iSiloIndex == aiSiloID && iLaunchIndex == aiLaunchID
    EndIf
    If IsRunning() && IsStageDone(300)
        Stop()
    EndIf
    If IsStageDone(300) || IsCompleted()
        Reset()
    EndIf
    iSiloIndex = aiSiloID
    iLaunchIndex = aiLaunchID
    bLaunchComplete = False
    Quest fleeQuest = Self as Quest
    If !fleeQuest.IsRunning()
        Keyword startKeyword = Game.GetFormFromFile(0x002D1150, "SeventySix.esm") as Keyword
        If startKeyword == None
            Return False
        EndIf
        Actor player = Game.GetPlayer()
        Bool eventStarted = startKeyword.SendStoryEventAndWait(akSiloLocation, player, player, aiSiloID, aiLaunchID)
        Int startPolls = 0
        While !fleeQuest.IsRunning() && (eventStarted || fleeQuest.IsStarting()) && startPolls < 40
            Utility.Wait(0.25)
            startPolls += 1
        EndWhile
        If !fleeQuest.IsRunning()
            Return False
        EndIf
    EndIf
    If TargetSiloLoc != None
        TargetSiloLoc.ForceLocationTo(akSiloLocation)
    EndIf
    If !fleeQuest.IsStageDone(10)
        fleeQuest.SetStage(10)
    EndIf
    Return fleeQuest.IsRunning()
EndFunction

Function SetCollectionEnabled(RefCollectionAlias akCollection, Bool abEnabled)
    If akCollection == None
        Return
    EndIf
    Int i = 0
    While i < akCollection.GetCount()
        ObjectReference targetRef = akCollection.GetAt(i)
        If targetRef != None
            If abEnabled
                targetRef.Enable()
            Else
                targetRef.Disable()
            EndIf
        EndIf
        i += 1
    EndWhile
EndFunction

Function SetDoorsSealed(Bool abSealed)
    If DoorsToSeal == None
        Return
    EndIf
    Int i = 0
    While i < DoorsToSeal.GetCount()
        ObjectReference doorRef = DoorsToSeal.GetAt(i)
        If doorRef != None
            If abSealed
                doorRef.SetOpen(False)
                doorRef.Lock(True, False)
            Else
                doorRef.Lock(False, False)
            EndIf
        EndIf
        i += 1
    EndWhile
EndFunction

Function HandleStage(Int aiStage)
    Quest fleeQuest = Self as Quest
    If aiStage == 10
        CacheLaunchReferences()
        bLaunchComplete = False
        iLaunchFailSafeCount = 0
        SetLaunchSoundPhase(1)
        fleeQuest.SetObjectiveDisplayed(iFleeObjID, True, True)
        Scene countdown = Game.GetFormFromFile(0x002D115A, "SeventySix.esm") as Scene
        If countdown != None && !countdown.IsPlaying()
            countdown.Start()
        EndIf
        ; INFO 05654F promises thirty seconds before the tube is sealed.
        StartTimer(30.0, iLaunchTimerID)
    ElseIf aiStage == 20
        SetDoorsSealed(True)
        SetLaunchSoundPhase(3)
    ElseIf aiStage == 30
        If B21LaunchRefs.Length == 0
            CacheLaunchReferences()
        EndIf
        SetLaunchSoundPhase(4)
        Int i = 0
        While i < B21LaunchRefs.Length
            Default1StateSyncActivator animation = B21LaunchRefs[i] as Default1StateSyncActivator
            If animation != None
                B21LaunchRefs[i].Enable()
                animation.BeginSyncAnimation()
                EN07_MissileSoundRefScript missileSound = B21LaunchRefs[i] as EN07_MissileSoundRefScript
                If missileSound != None
                    missileSound.BeginLaunchEffects()
                EndIf
            EndIf
            i += 1
        EndWhile
        If FXProjectileMissileICBMEngineStart != None && ExteriorMissile.GetReference() != None
            FXProjectileMissileICBMEngineStart.Play(ExteriorMissile.GetReference())
        EndIf
        StartTimer(0.1, iLaunchTimerID)
    ElseIf aiStage == 35
        SetCollectionEnabled(KillTriggers, True)
    ElseIf aiStage == 40
        SetCollectionEnabled(WindowsToSeal, True)
    ElseIf aiStage == 45
        If EN07_MQ_FleeSilo_0040_CountdownToReopening != None && !EN07_MQ_FleeSilo_0040_CountdownToReopening.IsPlaying()
            EN07_MQ_FleeSilo_0040_CountdownToReopening.Start()
        EndIf
    ElseIf aiStage == 200
        fleeQuest.SetObjectiveCompleted(iFleeObjID, True)
    ElseIf aiStage == 300
        bLaunchComplete = True
        CancelTimer(iLaunchTimerID)
        SetCollectionEnabled(KillTriggers, False)
        SetCollectionEnabled(WindowsToSeal, False)
        SetDoorsSealed(False)
        CleanupLaunchReferences()
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != iLaunchTimerID || bLaunchComplete || !IsRunning() || IsStageDone(300)
        Return
    EndIf
    Quest fleeQuest = Self as Quest
    If !fleeQuest.IsStageDone(20)
        fleeQuest.SetStage(20)
        StartTimer(1.0, iLaunchTimerID)
    ElseIf !fleeQuest.IsStageDone(30)
        fleeQuest.SetStage(30)
    Else
        Float progress = GetLaunchProgress()
        If progress < 0.0
            iLaunchFailSafeCount += 1
            If iLaunchFailSafeCount >= iLaunchFailSafeMax
                Debug.Trace("EN07: no missile animation binding; clearing local silo hazards")
                CleanupLaunchReferences()
                bLaunchComplete = True
                Return
            EndIf
            StartTimer(iLaunchProgressTimerLength, iLaunchTimerID)
            Return
        EndIf
        iLaunchFailSafeCount = 0
        If !fleeQuest.IsStageDone(35) && progress >= fNukeLaunchKillProgressTime
            fleeQuest.SetStage(iKillTriggersOnStage)
        EndIf
        If !fleeQuest.IsStageDone(40) && progress >= fNukeSealProgressTime
            fleeQuest.SetStage(iSealWindowsStage)
        EndIf
        If !fleeQuest.IsStageDone(45) && progress >= fDisableRocketSoundProgressTime
            StopLaunchSounds()
            fleeQuest.SetStage(45)
            fleeQuest.SetStage(iLaunchStage)
        EndIf
        If progress >= 1.0
            bLaunchComplete = True
        Else
            StartTimer(0.1, iLaunchTimerID)
        EndIf
    EndIf
EndEvent

Function FinishLocalLaunch()
    Quest fleeQuest = Self as Quest
    If fleeQuest.IsRunning() && !fleeQuest.IsStageDone(200)
        fleeQuest.SetStage(200)
    EndIf
EndFunction

Event OnQuestShutdown()
    CancelTimer(iLaunchTimerID)
    bLaunchComplete = True
    CleanupLaunchReferences()
    Scene countdown = Game.GetFormFromFile(0x002D115A, "SeventySix.esm") as Scene
    If countdown != None && countdown.IsPlaying()
        countdown.Stop()
    EndIf
    If EN07_MQ_FleeSilo_0040_CountdownToReopening != None && EN07_MQ_FleeSilo_0040_CountdownToReopening.IsPlaying()
        EN07_MQ_FleeSilo_0040_CountdownToReopening.Stop()
    EndIf
EndEvent

Function CacheCollectionReferences(RefCollectionAlias akCollection, ObjectReference[] akReferences)
    Int i = 0
    While akCollection != None && i < akCollection.GetCount()
        ObjectReference targetRef = akCollection.GetAt(i)
        If targetRef != None && akReferences.Find(targetRef) < 0
            akReferences.Add(targetRef)
        EndIf
        i += 1
    EndWhile
EndFunction

Function CacheLaunchReferences()
    B21LaunchRefs = new ObjectReference[0]
    B21LaunchRefs.Add(Missile.GetReference())
    B21LaunchRefs.Add(ExteriorMissile.GetReference())
    ObjectReference activeRef = ActiveMissile.GetReference()
    If activeRef != None && B21LaunchRefs.Find(activeRef) < 0
        B21LaunchRefs.Add(activeRef)
    EndIf
    B21SealedDoorRefs = new ObjectReference[0]
    CacheCollectionReferences(DoorsToSeal, B21SealedDoorRefs)
    B21ShutdownDisabledRefs = new ObjectReference[0]
    CacheCollectionReferences(KillTriggers, B21ShutdownDisabledRefs)
    CacheCollectionReferences(WindowsToSeal, B21ShutdownDisabledRefs)
    CacheCollectionReferences(GetAlias(22) as RefCollectionAlias, B21ShutdownDisabledRefs)
    CacheCollectionReferences(GetAlias(30) as RefCollectionAlias, B21ShutdownDisabledRefs)
    ReferenceAlias enableMarker = GetAlias(1) as ReferenceAlias
    If enableMarker != None && enableMarker.GetReference() != None
        B21ShutdownDisabledRefs.Add(enableMarker.GetReference())
    EndIf
    B21LaunchMarkers = new ObjectReference[0]
    Int i = 0
    While i < LaunchSoundMarkers.Length
        If LaunchSoundMarkers[i] != None && LaunchSoundMarkers[i].GetReference() != None
            B21LaunchMarkers.Add(LaunchSoundMarkers[i].GetReference())
        EndIf
        i += 1
    EndWhile
EndFunction

Float Function GetLaunchProgress()
    Int i = 0
    Float progress = -1.0
    While i < B21LaunchRefs.Length
        Default1StateSyncActivator animation = B21LaunchRefs[i] as Default1StateSyncActivator
        If animation != None
            Float candidate = animation.GetSyncAnimationProgress()
            If progress < 0.0 || candidate < progress
                progress = candidate
            EndIf
        EndIf
        i += 1
    EndWhile
    Return progress
EndFunction

Function StopLaunchSounds()
    Int i = 0
    While i < B21LaunchRefs.Length
        EN07_MissileSoundRefScript missileSound = B21LaunchRefs[i] as EN07_MissileSoundRefScript
        If missileSound != None
            missileSound.CancelLaunchEffects()
        EndIf
        i += 1
    EndWhile
    i = 0
    While i < B21LaunchMarkers.Length
        B21LaunchMarkers[i].Disable()
        i += 1
    EndWhile
EndFunction

Function SetLaunchSoundPhase(Int aiPhase)
    If aiPhase < 1 || aiPhase > 4 || IsStageDone(300)
        Return
    EndIf
    If aiPhase < 3 && (IsStageDone(20) || IsStageDone(30))
        Return
    EndIf
    If B21LaunchMarkers.Length == 0
        CacheLaunchReferences()
    EndIf
    Int i = 0
    While i < LaunchSoundMarkers.Length
        If LaunchSoundMarkers[i] != None
            ObjectReference markerRef = LaunchSoundMarkers[i].GetReference()
            If markerRef != None
                If i == aiPhase - 1
                    markerRef.Enable()
                Else
                    markerRef.Disable()
                EndIf
            EndIf
        EndIf
        i += 1
    EndWhile
EndFunction

Function CleanupLaunchReferences()
    StopLaunchSounds()
    Int i = 0
    While i < B21LaunchRefs.Length
        Default1StateSyncActivator animation = B21LaunchRefs[i] as Default1StateSyncActivator
        If animation != None
            animation.ResetSyncAnimation()
        EndIf
        i += 1
    EndWhile
    i = 0
    While i < B21ShutdownDisabledRefs.Length
        B21ShutdownDisabledRefs[i].Disable()
        i += 1
    EndWhile
    i = 0
    While i < B21SealedDoorRefs.Length
        B21SealedDoorRefs[i].Lock(False, False)
        i += 1
    EndWhile
EndFunction

Function ResetLocalSilo()
    Quest fleeQuest = Self as Quest
    If fleeQuest.IsRunning() && !fleeQuest.IsStageDone(300)
        fleeQuest.SetStage(300)
    Else
        HandleStage(300)
    EndIf
EndFunction
