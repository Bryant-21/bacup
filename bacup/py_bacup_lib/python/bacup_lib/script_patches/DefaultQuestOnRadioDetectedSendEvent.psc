Function RegisterRadioDetection()
    If !IsRunning() || RadioTransmitter == None || MyStoryManagerKeyword == None
        Return
    EndIf
    ObjectReference transmitter = RadioTransmitter.GetReference()
    If transmitter != None
        RegisterForRemoteEvent(transmitter, "OnPipboyRadioDetection")
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
    StartTimer(1.0, 1)
EndFunction

Event OnQuestInit()
    RegisterRadioDetection()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    RegisterRadioDetection()
EndEvent

Event ObjectReference.OnPipboyRadioDetection(ObjectReference akSender, Bool abDetected)
    If abDetected && RadioTransmitter != None && akSender == RadioTransmitter.GetReference()
        If SendQuestStoryEvent()
            CancelTimer(1)
        Else
            StartTimer(5.0, 1)
        EndIf
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != 1 || !IsRunning() || RadioTransmitter == None
        Return
    EndIf
    ObjectReference transmitter = RadioTransmitter.GetReference()
    If transmitter != None
        ; Out-of-range transmitters return FLT_MAX, not their physical distance.
        If transmitter.GetTransmitterDistance() < 300000000000000000000000000000000000000.0
            If SendQuestStoryEvent()
                Return
            EndIf
        EndIf
    EndIf
    StartTimer(5.0, 1)
EndEvent

Event OnQuestShutdown()
    CancelTimer(1)
    UnregisterForAllRemoteEvents()
EndEvent
