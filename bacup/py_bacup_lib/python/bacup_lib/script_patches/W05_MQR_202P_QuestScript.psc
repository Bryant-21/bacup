Event OnStageSet(Int auiStageID, Int auiItemID)
    ; Stages 1210 and 1220 are the QUST-declared turret activation fragments; the
    ; quest fragment enables each collection, this restores their bound targets.
    If auiStageID == 1210
        AssignTurretWaveTargets(Turrets01Aliases, Turrets01Targets)
    ElseIf auiStageID == 1220
        AssignTurretWaveTargets(Turrets02Aliases, Turrets02Targets)
    ElseIf auiStageID == 1501 || auiStageID == 1510
        TryStartLastVentPeek()
    ElseIf auiStageID == DefeatedBossStage
        StopBossVentCycle(False)
    ElseIf auiStageID == 930 || auiStageID == 940
        foodMenuPending = False
        CancelTimer(4)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == RaRaBossVentTimerID
        If IsStageDone(DefeatedBossStage)
            StopBossVentCycle(False)
        Else
            SelectNextBossVent()
        EndIf
    ElseIf aiTimerID == RaRaItemDropTimeOutTimerID
        If !IsRunning() || IsStageDone(DefeatedBossStage)
            Return
        EndIf
        If W05_MQR_202P_RaRaVent_1600_BossPeekSequence != None && W05_MQR_202P_RaRaVent_1600_BossPeekSequence.IsPlaying()
            W05_MQR_202P_RaRaVent_1600_BossPeekSequence.Stop()
        EndIf
        FinishBossPeekCycle()
    ElseIf aiTimerID == 3
        TryMoveRaRaToExitVent()
    ElseIf aiTimerID == 4
        TryOpenFoodTransfer()
    EndIf
EndEvent

Event OnQuestShutdown()
    StopBossVentCycle()
    CancelTimer(3)
    CancelTimer(4)
    foodMenuPending = False
    pendingExitVent = None
    pendingExitScene = None
EndEvent

Function RequestFoodTransfer()
    If !IsRunning() || !IsStageDone(900) || IsStageDone(930) || IsStageDone(940)
        Return
    EndIf
    If !IsStageDone(910)
        SetStage(910)
    EndIf
    foodMenuPending = True
    StartTimer(0.2, 4)
EndFunction

Function TryOpenFoodTransfer()
    If !foodMenuPending
        Return
    EndIf
    Actor raRaRef = None
    If RaRa != None
        raRaRef = RaRa.GetActorReference()
    EndIf
    If !IsRunning() || IsStageDone(930) || IsStageDone(940) || raRaRef == None
        foodMenuPending = False
        Return
    EndIf
    If raRaRef.IsTalking() || Utility.IsInMenuMode()
        StartTimer(0.2, 4)
        Return
    EndIf
    foodMenuPending = False
    raRaRef.OpenInventory(True)
EndFunction

Function EnableRaRaEntryVent(ReferenceAlias akVent)
    If IsRunning() && akVent != None
        akVent.TryToEnable()
    EndIf
EndFunction

Function HideRaRaInVent()
    If IsRunning() && RaRa != None
        RaRa.TryToDisable()
    EndIf
EndFunction

Function FinishLastVentEntry()
    If !IsRunning() || IsStageDone(RaRaUnPeekStage)
        Return
    EndIf
    If IsStageDone(1501)
        TryStartLastVentPeek()
        Return
    EndIf
    HideRaRaInVent()
    If IsRunning() && !IsStageDone(1501)
        SetStage(1501)
    EndIf
    TryStartLastVentPeek()
EndFunction

Function TryStartLastVentPeek()
    If !IsRunning() || !IsStageDone(1501) || !IsStageDone(1510) || IsStageDone(RaRaUnPeekStage)
        Return
    EndIf
    If W05_MQR_202P_RaRaVent_1500_PeekSequence02 != None && !W05_MQR_202P_RaRaVent_1500_PeekSequence02.IsPlaying()
        W05_MQR_202P_RaRaVent_1500_PeekSequence02.Start()
        MoveRaRaToExitVent(RaRaVent1510Peek, W05_MQR_202P_RaRaVent_1500_PeekSequence02)
    EndIf
EndFunction

Function DropEarlyPulseGrenade()
    If !IsRunning() || pulseGrenadeDropped || PulseGrenade == None || RaRa == None
        Return
    EndIf
    Actor raRaRef = RaRa.GetActorReference()
    If raRaRef == None
        Return
    EndIf
    pulseGrenadeDropped = True
    ObjectReference droppedGrenade = raRaRef.PlaceAtMe(PulseGrenade, 1, False, False, False)
    If droppedGrenade == None
        pulseGrenadeDropped = False
    EndIf
EndFunction

Function EvaluateRaRaPackage()
    If IsRunning() && RaRa != None
        Actor raRaRef = RaRa.GetActorReference()
        If raRaRef != None
            raRaRef.EvaluatePackage()
        EndIf
    EndIf
EndFunction

Function MoveRaRaToExitVent(ReferenceAlias akVent, Scene akSourceScene)
    CancelTimer(3)
    pendingExitVent = akVent
    pendingExitScene = akSourceScene
    TryMoveRaRaToExitVent()
EndFunction

Function TryMoveRaRaToExitVent()
    If !IsRunning() || pendingExitVent == None || pendingExitScene == None || !pendingExitScene.IsPlaying()
        pendingExitVent = None
        pendingExitScene = None
        Return
    EndIf
    Actor raRaRef
    If RaRa != None
        raRaRef = RaRa.GetActorReference()
    EndIf
    ReferenceAlias exitAlias = pendingExitVent
    Scene exitScene = pendingExitScene
    ObjectReference exitVent = exitAlias.GetReference()
    If raRaRef != None && exitVent != None
        exitVent.Enable()
        If exitVent.Is3DLoaded()
            If !raRaRef.Is3DLoaded()
                raRaRef.Disable()
                raRaRef.MoveTo(exitVent)
                raRaRef.Enable()
            EndIf
            If !IsRunning() || pendingExitVent != exitAlias || pendingExitScene != exitScene || !exitScene.IsPlaying()
                Return
            EndIf
            If raRaRef.SnapIntoInteraction(exitVent)
                If pendingExitVent == exitAlias && pendingExitScene == exitScene
                    pendingExitVent = None
                    pendingExitScene = None
                EndIf
                Return
            EndIf
        EndIf
    EndIf
    If IsRunning() && pendingExitVent == exitAlias && pendingExitScene == exitScene && exitScene.IsPlaying()
        StartTimer(1.0, 3)
    EndIf
EndFunction

Function StartBossVentCycle()
    StopBossVentCycle()
    RaRaDropCount = 0
    If !IsStageDone(DefeatedBossStage)
        StartTimer(VentTimerInit, RaRaBossVentTimerID)
    EndIf
EndFunction

Function BeginBossPeek()
    If !IsRunning() || IsStageDone(DefeatedBossStage)
        Return
    EndIf
    RaRaBossVentReadyToPeek = True
    MoveRaRaToExitVent(RaRaBossCurrentVent, W05_MQR_202P_RaRaVent_1600_BossPeekSequence)
EndFunction

Function DropBossVentItem()
    If !IsRunning() || IsStageDone(DefeatedBossStage)
        Return
    EndIf
    CancelTimer(RaRaItemDropTimeOutTimerID)
    StartTimer(RaRaItemDropTimerOutTimerLength, RaRaItemDropTimeOutTimerID)
    If RaRaDropCount >= RaRaDropCountMax || RaRaItemToDrop == None || RaRaItemToDrop.GetReference() != None || RaRa == None || W05_MQR_202P_LL_RaRaDropItemList == None
        Return
    EndIf

    Actor raRaRef = RaRa.GetActorReference()
    If raRaRef == None
        Return
    EndIf

    ObjectReference droppedItem = raRaRef.PlaceAtMe(W05_MQR_202P_LL_RaRaDropItemList, 1, False, True, False)
    If droppedItem == None
        Return
    EndIf

    RaRaItemToDrop.ForceRefTo(droppedItem)
    droppedItem.Enable()
    RaRaDropCount += 1
    If !IsStageDone(ItemDroppedObjective)
        SetStage(ItemDroppedObjective)
    Else
        SetObjectiveCompleted(ItemDroppedObjective, False)
        SetObjectiveDisplayed(ItemDroppedObjective, True, True)
    EndIf
EndFunction

Function EndBossPeek()
    RaRaBossVentReadyToPeek = False
    EvaluateRaRaPackage()
EndFunction

Function FinishBossPeekCycle()
    RaRaBossVentReadyToPeek = False
    CancelTimer(RaRaItemDropTimeOutTimerID)
    ClearDroppedItemAlias()
    If IsRunning() && !IsStageDone(DefeatedBossStage)
        HideRaRaInVent()
        ClearBossVentAliases()
        StartTimer(Utility.RandomInt(VentTimerMin, VentTimerMax), RaRaBossVentTimerID)
    EndIf
EndFunction

Function StopBossVentCycle(Bool abClearVent = True)
    CancelTimer(RaRaBossVentTimerID)
    CancelTimer(RaRaItemDropTimeOutTimerID)
    RaRaBossVentReadyToPeek = False
    If W05_MQR_202P_RaRaVent_1600_BossPeekSequence != None && W05_MQR_202P_RaRaVent_1600_BossPeekSequence.IsPlaying()
        W05_MQR_202P_RaRaVent_1600_BossPeekSequence.Stop()
    EndIf
    ClearDroppedItemAlias()
    If abClearVent
        ClearBossVentAliases()
    EndIf
EndFunction

Function SelectNextBossVent()
    If !IsRunning() || IsStageDone(DefeatedBossStage) || BossVentData == None || BossVentData.Length == 0 || RaRaBossCurrentVent == None
        Return
    EndIf

    Int checkedVentCount = 0
    BossVentDatum selectedVent
    ObjectReference ventRef
    While checkedVentCount < BossVentData.Length
        If VentIndex >= BossVentData.Length
            VentIndex = 0
        EndIf
        selectedVent = BossVentData[VentIndex]
        VentIndex += 1
        checkedVentCount += 1
        ventRef = None
        If selectedVent.Vent != None
            ventRef = selectedVent.Vent.GetReference()
        EndIf
        If ventRef != None
            RaRaBossCurrentVent.ForceRefTo(ventRef)
            ForceCurrentBossMarker(83, selectedVent.MarkerA)
            ForceCurrentBossMarker(84, selectedVent.MarkerB)
            ForceCurrentBossMarker(85, selectedVent.MarkerC)
            RaRaBossVentReadyToPeek = True
            If W05_MQR_202P_RaRaVent_1600_BossPeekSequence != None && !W05_MQR_202P_RaRaVent_1600_BossPeekSequence.IsPlaying()
                W05_MQR_202P_RaRaVent_1600_BossPeekSequence.Start()
            EndIf
            Return
        EndIf
    EndWhile
EndFunction

Function ForceCurrentBossMarker(Int aiCurrentAliasID, ReferenceAlias akSourceAlias)
    ReferenceAlias currentMarker = GetAlias(aiCurrentAliasID) as ReferenceAlias
    If currentMarker != None && akSourceAlias != None && akSourceAlias.GetReference() != None
        currentMarker.ForceRefTo(akSourceAlias.GetReference())
    EndIf
EndFunction

Function ClearDroppedItemAlias()
    If RaRaItemToDrop != None && RaRaItemToDrop.GetReference() != None
        RaRaItemToDrop.Clear()
    EndIf
EndFunction

Function ClearBossVentAliases()
    If RaRaBossCurrentVent != None && RaRaBossCurrentVent.GetReference() != None
        RaRaBossCurrentVent.Clear()
    EndIf
    ClearBossMarker(83)
    ClearBossMarker(84)
    ClearBossMarker(85)
EndFunction

Function ClearBossMarker(Int aiCurrentAliasID)
    ReferenceAlias currentMarker = GetAlias(aiCurrentAliasID) as ReferenceAlias
    If currentMarker != None && currentMarker.GetReference() != None
        currentMarker.Clear()
    EndIf
EndFunction

Function AssignTurretWaveTargets(ReferenceAlias[] akTurretAliases, ReferenceAlias[] akTargetAliases)
    If akTurretAliases == None || akTargetAliases == None
        Return
    EndIf
    Int pairIndex = 0
    Actor turretActor
    Actor targetActor
    While pairIndex < akTurretAliases.Length && pairIndex < akTargetAliases.Length
        turretActor = None
        targetActor = None
        If akTurretAliases[pairIndex] != None
            turretActor = akTurretAliases[pairIndex].GetActorReference()
        EndIf
        If akTargetAliases[pairIndex] != None
            targetActor = akTargetAliases[pairIndex].GetActorReference()
        EndIf
        If turretActor != None && targetActor != None && !turretActor.IsDead() && !targetActor.IsDead()
            turretActor.StartCombat(targetActor, True)
        EndIf
        pairIndex += 1
    EndWhile
EndFunction
