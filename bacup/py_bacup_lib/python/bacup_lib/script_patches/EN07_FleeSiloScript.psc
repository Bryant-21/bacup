Bool Function BeginLocalLaunch(Int aiSiloID, Int aiLaunchID, Location akSiloLocation)
    iSiloIndex = aiSiloID
    iLaunchIndex = aiLaunchID
    bLaunchComplete = False
    If akSiloLocation != None
        TargetSiloLoc.ForceLocationTo(akSiloLocation)
    EndIf
    Quest fleeQuest = Self as Quest
    Bool questWasRunning = fleeQuest.IsRunning()
    If !questWasRunning
        fleeQuest.Start()
    EndIf
    If akSiloLocation != None
        TargetSiloLoc.ForceLocationTo(akSiloLocation)
    EndIf
    If !fleeQuest.IsStageDone(10)
        fleeQuest.SetStage(10)
    ElseIf questWasRunning
        HandleStage(10)
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
        fleeQuest.SetObjectiveDisplayed(iFleeObjID, True, True)
        StartTimer(1.0, iLaunchTimerID)
    ElseIf aiStage == 20
        SetDoorsSealed(True)
    ElseIf aiStage == 30
        ObjectReference activeMissileRef = ActiveMissile.GetReference()
        If activeMissileRef != None
            activeMissileRef.Enable()
            activeMissileRef.PlayAnimation("Launch")
        EndIf
        ObjectReference missileRef = Missile.GetReference()
        If missileRef != None
            missileRef.Enable()
            missileRef.PlayAnimation("Launch")
        EndIf
        ObjectReference exteriorMissileRef = ExteriorMissile.GetReference()
        If exteriorMissileRef != None
            exteriorMissileRef.Enable()
            exteriorMissileRef.PlayAnimation("Launch")
        EndIf
        Int i = 0
        While i < LaunchSoundMarkers.Length
            EN07_MissileSoundRefScript soundMarker = LaunchSoundMarkers[i].GetReference() as EN07_MissileSoundRefScript
            If soundMarker != None
                soundMarker.TriggerLaunchSound()
            EndIf
            i += 1
        EndWhile
        FXProjectileMissileICBMEngineStart.Play(exteriorMissileRef)
    ElseIf aiStage == 35
        SetCollectionEnabled(KillTriggers, True)
    ElseIf aiStage == 40
        SetCollectionEnabled(WindowsToSeal, True)
    ElseIf aiStage == 45
        EN07_MQ_FleeSilo_0040_CountdownToReopening.Start()
    ElseIf aiStage == 200
        fleeQuest.SetObjectiveCompleted(iFleeObjID, True)
    ElseIf aiStage == 300
        SetCollectionEnabled(KillTriggers, False)
        SetCollectionEnabled(WindowsToSeal, False)
        SetDoorsSealed(False)
        ObjectReference activeMissileRef = ActiveMissile.GetReference()
        If activeMissileRef != None
            activeMissileRef.Disable()
        EndIf
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != iLaunchTimerID || bLaunchComplete
        Return
    EndIf
    Quest fleeQuest = Self as Quest
    Int currentStage = fleeQuest.GetStage()
    If currentStage < 20
        fleeQuest.SetStage(20)
        StartTimer(1.0, iLaunchTimerID)
    ElseIf currentStage < 30
        fleeQuest.SetStage(30)
        StartTimer(3.0, iLaunchTimerID)
    ElseIf currentStage < 35
        fleeQuest.SetStage(35)
        StartTimer(1.0, iLaunchTimerID)
    ElseIf currentStage < 40
        fleeQuest.SetStage(40)
        StartTimer(1.0, iLaunchTimerID)
    ElseIf currentStage < 45
        fleeQuest.SetStage(45)
        StartTimer(1.0, iLaunchTimerID)
    Else
        bLaunchComplete = True
        fleeQuest.SetStage(50)
    EndIf
EndEvent

Function FinishLocalLaunch()
    Quest fleeQuest = Self as Quest
    If fleeQuest.IsRunning() && !fleeQuest.IsStageDone(200)
        fleeQuest.SetStage(200)
    EndIf
EndFunction

Function ResetLocalSilo()
    Quest fleeQuest = Self as Quest
    If fleeQuest.IsRunning() && !fleeQuest.IsStageDone(300)
        fleeQuest.SetStage(300)
    Else
        HandleStage(300)
    EndIf
EndFunction
