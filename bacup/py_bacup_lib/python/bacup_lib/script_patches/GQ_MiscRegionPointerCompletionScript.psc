Event OnStageSet(Int auiStageID, Int auiItemID)
    If !EventSent && StageReachesMiscObjective(auiStageID)
        EventSent = True
    EndIf
EndEvent

Bool Function StageReachesMiscObjective(Int aiStageID)
    If StageToCompleteMiscObjective < 0
        Return False
    EndIf
    If CompleteOnHigherStages
        Return aiStageID >= StageToCompleteMiscObjective
    EndIf
    Return aiStageID == StageToCompleteMiscObjective
EndFunction

Bool Function IsMiscObjectiveComplete()
    If EventSent
        Return True
    EndIf
    If StageToCompleteMiscObjective < 0
        EventSent = IsCompleted()
    ElseIf CompleteOnHigherStages
        EventSent = GetStage() >= StageToCompleteMiscObjective
    Else
        EventSent = IsStageDone(StageToCompleteMiscObjective)
    EndIf
    Return EventSent
EndFunction
