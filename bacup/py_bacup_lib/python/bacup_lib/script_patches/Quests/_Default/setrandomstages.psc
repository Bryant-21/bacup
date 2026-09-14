Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID != TriggerStage || PossibleStages == None || PossibleStages.Length == 0 || NumToSet <= 0
        Return
    EndIf
    If TurnOffStage >= 0 && IsStageDone(TurnOffStage)
        Return
    EndIf

    Int startIndex = Utility.RandomInt(0, PossibleStages.Length - 1)
    Int stagesChecked = 0
    Int stagesSet = 0
    While stagesChecked < PossibleStages.Length && stagesSet < NumToSet
        Int candidateIndex = (startIndex + stagesChecked) % PossibleStages.Length
        Int candidateStage = PossibleStages[candidateIndex]
        If candidateStage >= 0 && !IsStageDone(candidateStage)
            SetStage(candidateStage)
            stagesSet += 1
        EndIf
        stagesChecked += 1
    EndWhile
EndEvent
