Event OnStageSet(Int auiStageID, Int auiItemID)
    Quest owner = Self as Quest
    B21:QuestTimer questTimer = owner as B21:QuestTimer
    If questTimer == None || TimeData == None
        Return
    EndIf
    Int index = 0
    While index < TimeData.Length
        TimeDatum datum = TimeData[index]
        If datum != None && datum.Stage == auiStageID
            questTimer.EnsureQuestTimerRemaining(datum.TimeRemaining * 60.0)
        EndIf
        index += 1
    EndWhile
EndEvent
