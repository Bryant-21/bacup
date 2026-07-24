State closing
    Event OnBeginState(String asOldState)
        parent.OnBeginState(asOldState)
        If Delay > 0.0
            StartTimer(Delay, SoundOffDelay)
        Else
            ObjectReference linkedAudio = GetLinkedRef()
            If linkedAudio != None
                linkedAudio.Disable(False)
            EndIf
        EndIf
    EndEvent

    Event OnTimer(Int aiTimerID)
        If aiTimerID == SoundOffDelay
            ObjectReference linkedAudio = GetLinkedRef()
            If linkedAudio != None
                linkedAudio.Disable(False)
            EndIf
        EndIf
    EndEvent
EndState

State open
    Event OnBeginState(String asOldState)
        parent.OnBeginState(asOldState)
        CancelTimer(SoundOffDelay)
        ObjectReference linkedAudio = GetLinkedRef()
        If linkedAudio != None
            linkedAudio.Enable(False)
        EndIf
    EndEvent
EndState

Auto State Initial
    Event OnInit()
        ObjectReference linkedAudio = GetLinkedRef()
        If linkedAudio != None
            If IsOpen
                linkedAudio.Enable(False)
            Else
                linkedAudio.Disable(False)
            EndIf
        EndIf
    EndEvent
EndState
