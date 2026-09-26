Function Fragment_Phase_01_End()
    B21:QuestVariables tourVariables = GetOwningQuest() as B21:QuestVariables
    If tourVariables == None
        Return
    EndIf
    Int stationsVisited = 0
    Int stationStage = 110
    While stationStage <= 180
        If tourVariables.IsStageDone(stationStage)
            stationsVisited += 1
        EndIf
        stationStage += 10
    EndWhile
    tourVariables.SetVariable("StationsVisited", stationsVisited as Float)
EndFunction
