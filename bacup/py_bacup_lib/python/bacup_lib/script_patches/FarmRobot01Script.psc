Event OnQuestInit()
    Quest owner = Self as Quest
    B21:QuestTimer eventTimer = owner as B21:QuestTimer
    If eventTimer != None
        RegisterForCustomEvent(eventTimer, "QuestTimerEnded")
    EndIf
EndEvent

Event B21:QuestTimer.QuestTimerEnded(B21:QuestTimer akSender, Var[] akArgs)
    ; FO76 failed the activity when its 1800 s timer ran out; stage 8900 carries the
    ; FailQuest flag but no TimerEnd flag, so nothing else routes the expiry.
    If IsRunning() && !IsStageDone(500) && !IsStageDone(8900)
        SetStage(8900)
    EndIf
EndEvent

Event OnQuestShutdown()
    UnregisterForAllEvents()
EndEvent
