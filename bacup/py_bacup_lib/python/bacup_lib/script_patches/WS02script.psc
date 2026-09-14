Event OnQuestInit()
    If !IsStageDone(ObjectiveStage)
        SetStage(ObjectiveStage)
    EndIf
EndEvent
