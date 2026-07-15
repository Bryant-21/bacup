Event OnActivate(ObjectReference akActionRef)
    If !bActive && (EN02_AlarmActive == None || HasKeyword(EN02_AlarmActive))
        bActive = True
        If iDisableTimerLength > 0
            StartTimer(iDisableTimerLength, iTimerID)
        Else
            FinishDisable()
        EndIf
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == iTimerID
        FinishDisable()
    EndIf
EndEvent

Function FinishDisable()
    DisableNoWait()
    bActive = False
EndFunction
