Event OnStageSet(Int auiStageID, Int auiItemID)
    Int index = 0
    While GameDaysPassedGlobalsAndQuestStages != None && index < GameDaysPassedGlobalsAndQuestStages.Length
        GlobalsDatum datum = GameDaysPassedGlobalsAndQuestStages[index]
        If datum != None && datum.QuestStage == auiStageID && datum.NextAllowed != None
            Float daysToAdd = 0.0
            If datum.DaysToAdd != None
                daysToAdd = datum.DaysToAdd.GetValue()
            EndIf
            datum.NextAllowed.SetValue(Utility.GetCurrentGameTime() + daysToAdd)
        EndIf
        index += 1
    EndWhile
EndEvent
