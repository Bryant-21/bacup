Function Fragment_Phase_01_End()
    Quest owningQuest = GetOwningQuest()
    If owningQuest != None && !owningQuest.IsStageDone(9000)
        ; Objective 10 reads StationsVisited/TotalStations; nothing else seeds the
        ; total, and the tour has eight station stages (110-180).
        B21:QuestVariables tourVariables = owningQuest as B21:QuestVariables
        If tourVariables != None
            tourVariables.SetVariable("TotalStations", 8.0)
        EndIf
        owningQuest.SetObjectiveDisplayed(10, True)
        owningQuest.SetObjectiveDisplayed(20, True)
    EndIf
EndFunction
