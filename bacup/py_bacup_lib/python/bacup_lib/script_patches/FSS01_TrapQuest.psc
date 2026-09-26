Function ApplyCooldown()
    If QuestNextAvailTime == None || QuestCooldown == None
        Return
    EndIf
    QuestNextAvailTime.SetValue(Utility.GetCurrentGameTime() + QuestCooldown.GetValue() * MinToExcelConst)
EndFunction

Function ArmEventShutdown(Float afSeconds)
    Float delay = afSeconds
    If delay < 1.0
        delay = 1.0
    EndIf
    CancelTimer(7401)
    StartTimer(delay, 7401)
EndFunction

Event OnQuestInit()
    Quest owner = Self as Quest
    B21:QuestTimer questTimer = owner as B21:QuestTimer
    If questTimer != None
        RegisterForCustomEvent(questTimer, "QuestTimerEnded")
    EndIf
EndEvent

Event B21:QuestTimer.QuestTimerEnded(B21:QuestTimer akSender, Var[] akArgs)
    If !IsRunning() || IsStageDone(900) || IsStageDone(950) || IsStageDone(1000)
        Return
    EndIf
    SetStage(900)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 7401 && !IsStopping() && !IsStopped()
        Stop()
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(7401)
    UnregisterForAllEvents()
EndEvent
