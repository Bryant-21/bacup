Function BeginLateJoinCountdown(Int aiSeconds = 0)
    Int seconds = aiSeconds
    If seconds <= 0
        seconds = TooLateToJoinTime
    EndIf
    If seconds <= 0 && TWZ07TimerGlobal != None
        seconds = TWZ07TimerGlobal.GetValue() as Int
    EndIf
    If seconds <= 0 || StageToSet <= 0 || IsStageDone(StageToSet)
        Return
    EndIf
    TooLateTimer = 25050
    CancelTimer(TooLateTimer)
    StartTimer(seconds as Float, TooLateTimer)
EndFunction

Function CancelLateJoinCountdown()
    If TooLateTimer != 0
        CancelTimer(TooLateTimer)
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If TooLateTimer == 0 || aiTimerID != TooLateTimer || StageToSet <= 0
        Return
    EndIf
    If IsRunning() && !IsStageDone(StageToSet)
        SetStage(StageToSet)
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelLateJoinCountdown()
EndEvent
