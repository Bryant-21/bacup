Event OnStageSet(Int auiStageID, Int auiItemID)
    ; Stages 1210 and 1220 are the QUST-declared turret activation fragments; the
    ; quest fragment enables each collection, this restores their bound targets.
    If auiStageID == 1210
        AssignTurretWaveTargets(Turrets01Aliases, Turrets01Targets)
    ElseIf auiStageID == 1220
        AssignTurretWaveTargets(Turrets02Aliases, Turrets02Targets)
    ElseIf auiStageID == DefeatedBossStage
        StopBossVentCycle()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == RaRaBossVentTimerID
        If IsStageDone(DefeatedBossStage)
            StopBossVentCycle()
        Else
            SelectNextBossVent()
        EndIf
    ElseIf aiTimerID == RaRaItemDropTimeOutTimerID
        If W05_MQR_202P_RaRaVent_1600_BossPeekSequence != None && W05_MQR_202P_RaRaVent_1600_BossPeekSequence.IsPlaying()
            W05_MQR_202P_RaRaVent_1600_BossPeekSequence.Stop()
        EndIf
        FinishBossPeekCycle()
    EndIf
EndEvent

Event OnQuestShutdown()
    StopBossVentCycle()
EndEvent

Function StartBossVentCycle()
    StopBossVentCycle()
    RaRaDropCount = 0
    If !IsStageDone(DefeatedBossStage)
        StartTimer(VentTimerInit, RaRaBossVentTimerID)
    EndIf
EndFunction

Function BeginBossPeek()
    RaRaBossVentReadyToPeek = True
EndFunction

Function DropBossVentItem()
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
    EndIf
EndFunction

Function EndBossPeek()
    RaRaBossVentReadyToPeek = False
EndFunction

Function FinishBossPeekCycle()
    RaRaBossVentReadyToPeek = False
    CancelTimer(RaRaItemDropTimeOutTimerID)
    ClearDroppedItemAlias()
    ClearBossVentAliases()
    If !IsStageDone(DefeatedBossStage)
        StartTimer(Utility.RandomInt(VentTimerMin, VentTimerMax), RaRaBossVentTimerID)
    EndIf
EndFunction

Function StopBossVentCycle()
    CancelTimer(RaRaBossVentTimerID)
    CancelTimer(RaRaItemDropTimeOutTimerID)
    RaRaBossVentReadyToPeek = False
    If W05_MQR_202P_RaRaVent_1600_BossPeekSequence != None && W05_MQR_202P_RaRaVent_1600_BossPeekSequence.IsPlaying()
        W05_MQR_202P_RaRaVent_1600_BossPeekSequence.Stop()
    EndIf
    ClearDroppedItemAlias()
    ClearBossVentAliases()
EndFunction

Function SelectNextBossVent()
    If BossVentData == None || BossVentData.Length == 0 || RaRaBossCurrentVent == None
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
