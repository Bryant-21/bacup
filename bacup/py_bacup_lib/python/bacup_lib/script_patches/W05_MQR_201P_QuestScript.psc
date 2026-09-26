Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID >= 1420 && auiStageID < Deathtrap03BeginStage
        WatchDeathtrap03Trigger()
    EndIf
    If auiStageID == Deathtrap03BeginStage
        StartTimer(WeaselDeathtrapSayTimerLength, WeaselDeathtrapSayTimerID)
    ElseIf auiStageID == StopDetonationStage
        StartTimer(DetectionTimerLength, DetectionTimerID)
    ElseIf auiStageID == LouNoticedStage || auiStageID == AllBreakersOffStage || auiStageID == PlayerTalkedToLouStage
        CancelTimer(DetectionTimerID)
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(DetectionTimerID)
    CancelTimer(WeaselDeathtrapSayTimerID)
    CancelTimer(SignalTimerID)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == WeaselDeathtrapSayTimerID
        Actor weaselActor = None
        If Weasel != None
            weaselActor = Weasel.GetActorReference()
        EndIf
        If weaselActor != None && W05_MQR_201P_Weasel_Deathtrap03_Comment02 != None
            weaselActor.Say(W05_MQR_201P_Weasel_Deathtrap03_Comment02, weaselActor, False, Game.GetPlayer())
        EndIf
        If !IsStageDone(Deathtrap03WeaselStage)
            SetStage(Deathtrap03WeaselStage)
        EndIf
    ElseIf aiTimerID == DetectionTimerID
        Actor louActor = None
        Actor playerActor = None
        If Lou != None
            louActor = Lou.GetActorReference()
        EndIf
        If currentPlayer != None
            playerActor = currentPlayer.GetActorReference()
        EndIf
        If playerActor == None
            playerActor = Game.GetPlayer()
        EndIf
        If IsStageDone(LouNoticedStage) || IsStageDone(AllBreakersOffStage) || IsStageDone(PlayerTalkedToLouStage)
            Return
        EndIf
        If louActor == None || playerActor == None || louActor.IsDead()
            Return
        EndIf
        If playerActor.GetDistance(louActor) <= LouDistance && playerActor.IsDetectedBy(louActor)
            If W05_MQR_201P_LouSaysTopic_SneakFail != None
                louActor.Say(W05_MQR_201P_LouSaysTopic_SneakFail, louActor, False, playerActor)
            EndIf
            SetStage(LouNoticedStage)
            Return
        EndIf
        StartTimer(DetectionTimerLength, DetectionTimerID)
    ElseIf aiTimerID == SignalTimerID
        If !bSignalActive || !IsRunning()
            Return
        EndIf
        ShowSignalStrength()
        StartTimer(SignalTimerLength, SignalTimerID)
    EndIf
EndEvent

; DeathTrap03BTrigger has no script of its own; the stripped server quest script set 1510 when
; the player walked into it, and 1520/1530/1600 all require 1510.
Function WatchDeathtrap03Trigger()
    If IsStageDone(Deathtrap03BeginStage) || DeathTrap03BTrigger == None
        Return
    EndIf
    ObjectReference triggerRef = DeathTrap03BTrigger.GetReference()
    If triggerRef != None
        RegisterForRemoteEvent(triggerRef, "OnTriggerEnter")
    EndIf
EndFunction

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
    If DeathTrap03BTrigger == None || akSender != DeathTrap03BTrigger.GetReference() || akActionRef != Game.GetPlayer()
        Return
    EndIf
    UnregisterForRemoteEvent(akSender, "OnTriggerEnter")
    If IsRunning() && IsStageDone(1420) && !IsStageDone(Deathtrap03BeginStage)
        SetStage(Deathtrap03BeginStage)
    EndIf
EndEvent

Function StartSignalTracking()
    bSignalActive = True
    fDoorToWeaselDistance = 0.0
    CancelTimer(SignalTimerID)
    StartTimer(SignalTimerLength, SignalTimerID)
EndFunction

Function StopSignalTracking()
    bSignalActive = False
    CancelTimer(SignalTimerID)
EndFunction

Function ShowSignalStrength()
    If W05_MQR_201P_SignalStrengthMessage == None || !Game.IsPlayerRadioOn()
        Return
    EndIf
    If Math.Abs(Game.GetPlayerRadioFrequency() - RadioStationFreq) >= 0.05
        Return
    EndIf
    Float signalDistance = GetSignalDistance()
    If signalDistance < 0.0
        Return
    EndIf
    ; The server formula is stripped; the beacon scene's outer beep band is 36000 units.
    Float signalStrength = 100.0 * (1.0 - signalDistance / 36000.0)
    If signalStrength < 0.0
        signalStrength = 0.0
    ElseIf signalStrength > 100.0
        signalStrength = 100.0
    EndIf
    W05_MQR_201P_SignalStrengthMessage.Show(signalStrength)
EndFunction

; Distance from the player to the tracker. Inside the mine it is the direct
; distance to Weasel; outside it is the distance to the entrance marker plus the
; entrance-to-Weasel offset, so the reading is continuous through the door.
; Returns -1 when nothing can be measured.
Float Function GetSignalDistance()
    Actor playerActor = None
    If currentPlayer != None
        playerActor = currentPlayer.GetActorReference()
    EndIf
    If playerActor == None
        playerActor = Game.GetPlayer()
    EndIf
    ObjectReference weaselRef = None
    If Weasel != None
        weaselRef = Weasel.GetReference()
    EndIf
    If playerActor == None
        Return -1.0
    EndIf

    Location mineInterior = LocMountainsLousMineInteriorLocation
    If mineInterior == None && LousMineInteriorLoc != None
        mineInterior = LousMineInteriorLoc.GetLocation()
    EndIf
    If weaselRef != None && mineInterior != None && playerActor.IsInLocation(mineInterior)
        Return playerActor.GetDistance(weaselRef)
    EndIf

    ObjectReference markerRef = None
    If FisherRadioMarker != None
        markerRef = FisherRadioMarker.GetReference()
    EndIf
    If markerRef == None
        Return -1.0
    EndIf
    If fDoorToWeaselDistance <= 0.0 && weaselRef != None && LouMineEntranceDoorInterior != None
        ObjectReference doorRef = LouMineEntranceDoorInterior.GetReference()
        If doorRef != None
            Float doorOffset = doorRef.GetDistance(weaselRef)
            If doorOffset > 0.0 && doorOffset < 36000.0
                fDoorToWeaselDistance = doorOffset
            EndIf
        EndIf
    EndIf
    Return playerActor.GetDistance(markerRef) + fDoorToWeaselDistance
EndFunction
