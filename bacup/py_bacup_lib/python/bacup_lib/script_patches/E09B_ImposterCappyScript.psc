Event OnLoad()
    If LaughInterval > 0
        StartTimer(LaughInterval as Float, 0)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 0
        Laugh()
        If LaughInterval > 0
            StartTimer(LaughInterval as Float, 0)
        EndIf
    EndIf
EndEvent

Event OnUnload()
    StopLaughSound()
EndEvent
