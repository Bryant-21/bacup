Event OnUnload()
    If !HasKeyword(PreventRelockKeyword)
        StartTimer(iCountdownTimerLength, iCountdownTimerID)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == iCountdownTimerID
        Lock()
    EndIf
EndEvent
