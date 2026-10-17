; Single-player controller for RE_Scene_MTNZ05_Messenger (01A634). FO76's server owned it;
; Server76 only moved the messenger. Stage flow is carried by the scenes themselves:
; IntroScene -> 200, StartEscortScene -> 300, StartPackageScene (travel) -> 400,
; DeliveryScene (started by the stage 400 fragment) -> 500. This script supplies what
; the scenes cannot: the requested part, taking it at the 210 greeting, and failure when
; the messenger dies. Timer 6 is free (the property timer IDs are 1-5); the escort time
; limit stays with B21:ObjectiveTimers (objective 300 -> stage 600).

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 200
        SpawnRequestedPart()
    ElseIf auiStageID == PartGreetingStage
        TakePartAndStartEscort()
    ElseIf auiStageID == EscortStage
        StartTravel()
    ElseIf auiStageID == FailureStage
        PlayFailure()
    EndIf
EndEvent

Function SpawnRequestedPart()
    If MTNZ05_PartsList == None || MTNZ05_PartsList.GetSize() == 0
        Return
    EndIf
    iRandomPart = Utility.RandomInt(0, MTNZ05_PartsList.GetSize() - 1)
    TargetPart = MTNZ05_PartsList.GetAt(iRandomPart) as MiscObject
    If TargetPart == None || PartMarkers == None || PartMarkers.Length == 0
        Return
    EndIf
    Int start = Utility.RandomInt(0, PartMarkers.Length - 1)
    Int offset = 0
    While offset < PartMarkers.Length
        ReferenceAlias markerAlias = PartMarkers[(start + offset) % PartMarkers.Length]
        ObjectReference marker = None
        If markerAlias != None
            marker = markerAlias.GetReference()
        EndIf
        If marker != None
            ObjectReference part = marker.PlaceAtMe(TargetPart, 1, False, False, True)
            If part != None
                PartMarker.ForceRefTo(marker)
                SelectedRobotPart.ForceRefTo(part)
                Return
            EndIf
        EndIf
        offset += 1
    EndWhile
EndFunction

Function TakePartAndStartEscort()
    Actor player = Game.GetPlayer()
    ObjectReference messenger = HandyMessenger.GetReference()
    If TargetPart != None && player.GetItemCount(TargetPart) > 0
        player.RemoveItem(TargetPart, 1, False, messenger)
    EndIf
    If messenger != None && NPCRobotMrHandyRepair != None
        NPCRobotMrHandyRepair.Play(messenger)
    EndIf
    Scene escortScene = Game.GetFormFromFile(0x003C4742, "SeventySix.esm") as Scene
    If escortScene != None && !escortScene.IsPlaying()
        escortScene.Start()
    ElseIf !IsStageDone(EscortStage)
        SetStage(EscortStage)
    EndIf
EndFunction

Function StartTravel()
    Scene travelScene = Game.GetFormFromFile(0x0004E5EE, "SeventySix.esm") as Scene
    If travelScene != None && !travelScene.IsPlaying()
        travelScene.Start()
    EndIf
    StartTimer(5.0, 6)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != 6 || !IsRunning() || IsStageDone(DeliveredStage) || IsStageDone(FailureStage)
        Return
    EndIf
    Actor messenger = HandyMessenger.GetActorReference()
    If messenger == None || messenger.IsDead()
        SetStage(FailureStage)
        Return
    EndIf
    StartTimer(5.0, 6)
EndEvent

Function PlayFailure()
    CancelTimer(6)
    Actor messenger = HandyMessenger.GetActorReference()
    If messenger != None && !messenger.IsDead() && MTNZ05_Messenger_FailureScene != None
        MTNZ05_Messenger_FailureScene.Start()
    EndIf
    ObjectReference part = SelectedRobotPart.GetReference()
    If part != None && part.GetContainer() == None
        part.Disable()
        part.Delete()
    EndIf
EndFunction
