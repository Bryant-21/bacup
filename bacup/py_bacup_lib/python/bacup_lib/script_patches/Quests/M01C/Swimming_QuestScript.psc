; The FO76 quest script is server-stripped. It drives the swimsuit check, hands the buoy
; course to the player alias once the instructor's start scene sets 200, and closes the
; test when the player swims back to the instruction station (400).
Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 100
        SetObjectiveDisplayed(10, True)
        SetStage(150)
    ElseIf auiStageID == 150
        EvaluateSwimsuit()
    ElseIf auiStageID == Swimsuit_Off_Stage
        SetObjectiveDisplayed(40, True)
        SetObjectiveDisplayed(999, True)
        RegisterForRemoteEvent(Game.GetPlayer(), "OnItemEquipped")
    ElseIf auiStageID == 200
        UnregisterForRemoteEvent(Game.GetPlayer(), "OnItemEquipped")
        SetObjectiveCompleted(10, True)
        SetObjectiveCompleted(40, True)
        SetObjectiveCompleted(45, True)
        Quests:M01C:Swimming_PlayerScript swimmer = Player_Alias as Quests:M01C:Swimming_PlayerScript
        If swimmer
            swimmer.BeginSwimTest()
        EndIf
    ElseIf auiStageID == 300
        SetObjectiveCompleted(50, True)
        SetObjectiveDisplayed(60, True)
    ElseIf auiStageID == 400
        SetObjectiveCompleted(60, True)
        SetObjectiveCompleted(20, True)
        If !IsStageDone(9000)
            SetStage(9000)
        EndIf
        Scene resultsScene = Game.GetFormFromFile(0x004178D3, "SeventySix.esm") as Scene
        If resultsScene && !resultsScene.IsPlaying()
            resultsScene.Start()
        EndIf
    ElseIf auiStageID == 9001
        Stop()
    EndIf
EndEvent

Function EvaluateSwimsuit()
    Actor playerRef = Game.GetPlayer()
    If SwimsuitItem && playerRef.IsEquipped(SwimsuitItem)
        SetStage(Swimsuit_On_Stage)
    Else
        SetStage(Swimsuit_Off_Stage)
    EndIf
EndFunction

Event Actor.OnItemEquipped(Actor akSender, Form akBaseObject, ObjectReference akReference)
    If akBaseObject != SwimsuitItem || !IsStageDone(Swimsuit_Off_Stage) || IsStageDone(SwimTest_StartStage) || IsStageDone(200)
        Return
    EndIf
    SetObjectiveCompleted(40, True)
    SetObjectiveDisplayed(45, True)
    SetStage(SwimTest_StartStage)
EndEvent

Event OnQuestShutdown()
    UnregisterForAllRemoteEvents()
EndEvent
