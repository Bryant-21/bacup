Event OnTriggerEnter(ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || akActionRef != playerRef || ScanInCooldown
        Return
    EndIf
    If W05_MQ_004P_Crane == None || W05_MQ_004P_Crane_PlayerRegisteredPipBoy == None
        Return
    EndIf
    If !W05_MQ_004P_Crane.IsRunning() && !W05_MQ_004P_Crane.IsCompleted()
        Return
    EndIf

    ScanInCooldown = True
    StartTimer(ScanCooldownLength, ScanCooldownID)

    If W05_MQ_004P_Crane.IsRunning() && !W05_MQ_004P_Crane.IsCompleted() && StageToSetOnApproach > 0 && !W05_MQ_004P_Crane.IsStageDone(StageToSetOnApproach)
        W05_MQ_004P_Crane.SetStage(StageToSetOnApproach)
    EndIf

    ObjectReference scannerRef = None
    If W05_MQ_004P_Crane_CacheScannerKeyword != None
        scannerRef = GetLinkedRef(W05_MQ_004P_Crane_CacheScannerKeyword)
    EndIf
    If playerRef.GetValue(W05_MQ_004P_Crane_PlayerRegisteredPipBoy) > 0.0
        If W05_MQ_004P_Crane_CacheDoorKeyword == None
            Return
        EndIf
        ObjectReference cacheDoor = GetLinkedRef(W05_MQ_004P_Crane_CacheDoorKeyword)
        If cacheDoor == None
            Return
        EndIf
        If !W05_MQ_004P_Crane.IsCompleted() && StageToSetOnOpen > 0 && !W05_MQ_004P_Crane.IsStageDone(StageToSetOnOpen)
            If !W05_MQ_004P_Crane.SetStage(StageToSetOnOpen)
                Return
            EndIf
        EndIf
        cacheDoor.Unlock()
        cacheDoor.SetOpen(True)
        If scannerRef != None && W05_MQ_004P_Crane_AccessGranted != None
            scannerRef.Say(W05_MQ_004P_Crane_AccessGranted, akTarget = playerRef)
        EndIf
    ElseIf scannerRef != None && W05_MQ_004P_Crane_AccessDenied != None
        scannerRef.Say(W05_MQ_004P_Crane_AccessDenied, akTarget = playerRef)
    EndIf
EndEvent

Event OnCellLoad()
    CancelTimer(ScanCooldownID)
    ScanInCooldown = False
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == ScanCooldownID
        ScanInCooldown = False
    EndIf
EndEvent
