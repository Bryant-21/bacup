Event OnTriggerEnter(ObjectReference akSenderRef, ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If akActionRef != playerRef || bProcessing
        Return
    EndIf
    If playerRef.GetValue(W05_MQ_004P_Crane_PlayerRegisteredPipBoy) > 0.0
        Return
    EndIf

    bProcessing = True
    Int index = 0
    While index < Turrets.GetCount()
        Actor turretRef = Turrets.GetActorAt(index)
        If turretRef != None && !turretRef.IsDead()
            turretRef.EvaluatePackage()
            turretRef.StartCombat(playerRef, True)
        EndIf
        index += 1
    EndWhile

    If !InCooldown
        ObjectReference speakerRef = Loudspeaker.GetReference()
        If speakerRef != None
            speakerRef.Say(W05_MQ_004P_Crane_UnregisteredIntruder, akTarget = playerRef)
        EndIf
        InCooldown = True
        StartTimer(CooldownTimerLength, CooldownTimerID)
    EndIf
    bProcessing = False
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == CooldownTimerID
        InCooldown = False
    EndIf
EndEvent
