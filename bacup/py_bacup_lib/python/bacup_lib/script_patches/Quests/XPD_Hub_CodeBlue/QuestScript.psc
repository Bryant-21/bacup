Event OnQuestInit()
    myPlayer = Alias_Player.GetActorReference()
EndEvent

Function SelectScenario()
    If myPlayer == None || ScenarioData == None || ScenarioData.Length == 0
        Return
    EndIf

    Int scenarioIndex = Utility.RandomInt(0, ScenarioData.Length - 1)
    If ScenarioData.Length > 1 && XPD_Hub_CodeBlueScenarioID_Previous != None
        Int previousIndex = myPlayer.GetValue(XPD_Hub_CodeBlueScenarioID_Previous) as Int
        If scenarioIndex == previousIndex
            scenarioIndex = (scenarioIndex + 1) % ScenarioData.Length
        EndIf
    EndIf

    chosenScenario = ScenarioData[scenarioIndex]
    If XPD_Hub_CodeBlueScenarioID_AV != None
        myPlayer.SetValue(XPD_Hub_CodeBlueScenarioID_AV, chosenScenario.ID)
    EndIf
    If XPD_Hub_CodeBlueScenarioID_Previous != None
        myPlayer.SetValue(XPD_Hub_CodeBlueScenarioID_Previous, chosenScenario.ID)
    EndIf
    If Loc_Current != None && chosenScenario.ScenarioLoc != None
        Loc_Current.ForceLocationTo(chosenScenario.ScenarioLoc.GetLocation())
    EndIf
    If Terminal_Current != None && chosenScenario.Terminals != None && chosenScenario.Terminals.GetCount() > 0
        Terminal_Current.ForceRefTo(chosenScenario.Terminals.GetAt(0))
    EndIf
    If Container_RandomQuestContainer != None && chosenScenario.Containers != None && chosenScenario.Containers.GetCount() > 0
        Container_RandomQuestContainer.ForceRefTo(chosenScenario.Containers.GetAt(0))
    EndIf
EndFunction

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 50
        SetObjectiveDisplayed(10)
        CancelTimer(1)
        StartTimer(3.0, 1)
    ElseIf auiStageID == 100
        SelectScenario()
        SetObjectiveCompleted(10)
        If !IsStageDone(110)
            SetStage(110)
        EndIf
    ElseIf auiStageID == 110
        SetObjectiveDisplayed(50)
    ElseIf auiStageID == 200
        SetObjectiveCompleted(50)
        SetObjectiveDisplayed(60)
        If chosenScenario.RetrieveItemObjective >= 0
            SetObjectiveDisplayed(chosenScenario.RetrieveItemObjective)
        EndIf
    ElseIf auiStageID == 205
        SetObjectiveCompleted(50)
        SetObjectiveDisplayed(60)
        If chosenScenario.RetrieveItemObjective >= 0
            SetObjectiveDisplayed(chosenScenario.RetrieveItemObjective)
        EndIf
    ElseIf auiStageID == 210
        SetObjectiveCompleted(60)
        If chosenScenario.RetrieveItemObjective >= 0
            SetObjectiveCompleted(chosenScenario.RetrieveItemObjective)
        EndIf
        SetObjectiveDisplayed(500)
    ElseIf auiStageID == 500
        SetObjectiveCompleted(500)
        If !IsStageDone(9000)
            SetStage(9000)
        EndIf
    ElseIf auiStageID == 8000
        CancelTimer(1)
        Stop()
    ElseIf auiStageID == 9000
        CancelTimer(1)
        CompleteAllObjectives()
    ElseIf auiStageID == 10000
        Stop()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != 1 || !IsRunning()
        Return
    EndIf
    CheckClinicHandover()
    If IsRunning() && !IsStageDone(9000)
        StartTimer(3.0, 1)
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(1)
EndEvent

Bool Function PlayerIsBeside(Int aiAliasID)
    If myPlayer == None
        myPlayer = Alias_Player.GetActorReference()
    EndIf
    ReferenceAlias target = GetAlias(aiAliasID) as ReferenceAlias
    If myPlayer == None || target == None
        Return False
    EndIf
    ObjectReference ref = target.GetReference()
    Return ref != None && myPlayer.GetDistance(ref) <= 384.0
EndFunction

; The converted Rucker and Responder Medic dialogue cannot run, so standing with
; them stands in for accepting the assignment and for handing the records in.
Function CheckClinicHandover()
    If !IsStageDone(100)
        If PlayerIsBeside(2)
            SetStage(100)
        EndIf
    ElseIf IsStageDone(210) && !IsStageDone(500)
        If PlayerIsBeside(28)
            SetStage(500)
        EndIf
    EndIf
EndFunction
