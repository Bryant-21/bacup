Event OnTriggerEnter(ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If akActionRef != playerRef || ScanInCooldown
        Return
    EndIf

    ScanInCooldown = True
    StartTimer(ScanCooldownLength, ScanCooldownID)

    If StageToSetOnApproach > 0 && !W05_MQ_004P_Crane.IsStageDone(StageToSetOnApproach)
        W05_MQ_004P_Crane.SetStage(StageToSetOnApproach)
    EndIf

    ObjectReference scannerRef = GetLinkedRef(W05_MQ_004P_Crane_CacheScannerKeyword)
    If playerRef.GetValue(W05_MQ_004P_Crane_PlayerRegisteredPipBoy) > 0.0
        If scannerRef != None
            scannerRef.Say(W05_MQ_004P_Crane_AccessGranted, akTarget = playerRef)
        EndIf
        ObjectReference cacheDoor = GetLinkedRef(W05_MQ_004P_Crane_CacheDoorKeyword)
        If cacheDoor != None
            cacheDoor.SetOpen(True)
        EndIf
        If StageToSetOnOpen > 0 && !W05_MQ_004P_Crane.IsStageDone(StageToSetOnOpen)
            W05_MQ_004P_Crane.SetStage(StageToSetOnOpen)
        EndIf
    ElseIf scannerRef != None
        scannerRef.Say(W05_MQ_004P_Crane_AccessDenied, akTarget = playerRef)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == ScanCooldownID
        ScanInCooldown = False
    EndIf
EndEvent
