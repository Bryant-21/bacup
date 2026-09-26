Event OnQuestInit()
    Quest owner = Self as Quest
    B21:QuestTimer questTimer = owner as B21:QuestTimer
    If questTimer != None
        RegisterForCustomEvent(questTimer, "QuestTimerEnded")
    EndIf
EndEvent

Event B21:QuestTimer.QuestTimerEnded(B21:QuestTimer akSender, Var[] akArgs)
    If iShutDownStage >= 0 && GetStageDone(iShutDownStage)
        Return
    EndIf
    If iStageToSet >= 0 && !GetStageDone(iStageToSet)
        SetStage(iStageToSet)
    EndIf
EndEvent

Event OnQuestShutdown()
    Quest owner = Self as Quest
    B21:QuestTimer questTimer = owner as B21:QuestTimer
    If questTimer != None
        UnregisterForCustomEvent(questTimer, "QuestTimerEnded")
    EndIf
EndEvent
