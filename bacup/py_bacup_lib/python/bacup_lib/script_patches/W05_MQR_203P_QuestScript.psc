Event OnQuestInit()
    If !IsStageDone(2)
        SetStage(2)
    EndIf
    If !IsStageDone(100)
        SetStage(100)
    EndIf
EndEvent
