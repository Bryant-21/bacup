Event OnQuestInit()
    ; EventSent is a plain script variable and survives the quest stop, so on a daily's
    ; second run the pointer would report "already complete" before the player did
    ; anything. Carriers: FF05_Balance 01C035, SFZ03_Queen 0451FC, SFZ04_Waste 124487,
    ; SFS02_Play 18C91A, TW010 23C7F3.
    EventSent = False
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If !EventSent && (StageReachesMiscObjective(auiStageID) || IsCompleted())
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
        ; A completed quest's region pointer must not stay open even when the exact
        ; stage was skipped: FF05_Balance 01C035 wants stage 15, but its own start
        ; fragment jumps a repeat run straight to 50 and 15 is never set again.
        EventSent = IsStageDone(StageToCompleteMiscObjective) || IsCompleted()
    EndIf
    Return EventSent
EndFunction
