; TODO

Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_204P_Started, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0015_Item_00()
    SetObjectiveDisplayed(15)
    ObjectReference vaultMarker = Alias_EnableMarkerVault79.GetReference()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If vaultMarker != None && playerRef != None && W05_MQS_204P_ActorEnableMarker != None
        vaultMarker.Enable()
        W05_MQS_204P_ActorEnableMarker.Enable()
        If GetStage() < 90
            SetStage(90)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0090_Item_00()
    SetObjectiveDisplayed(90)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0200_Item_00()
    If GetStage() < 250
        SetStage(250)
    EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && playerRef.GetItemCount(W05_MQS_204P_TheMazeID) < 1
        playerRef.AddItem(W05_MQS_204P_TheMazeID, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    If W05_MQS_204P_HijackWelcomeScene != None
        W05_MQS_204P_HijackWelcomeScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0360_Item_00()
    If !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0370_Item_00()
    If !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0390_Item_00()
    If !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0525_Item_00()
    If W05_MQS_204P_PennyUseDoorTerminalScene != None
        W05_MQS_204P_PennyUseDoorTerminalScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0550_Item_00()
    ObjectReference doorRef = Alias_MazeDoor01.GetReference()
    If doorRef != None
        doorRef.SetOpen(True)
    EndIf
    doorRef = Alias_MazeDoor02.GetReference()
    If doorRef != None
        doorRef.SetOpen(True)
    EndIf
    doorRef = Alias_MazeDoor03.GetReference()
    If doorRef != None
        doorRef.SetOpen(True)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    ObjectReference wallRef = Alias_FakeWall01.GetReference()
    If wallRef != None
        wallRef.Disable()
    EndIf
    wallRef = Alias_FakeWall02.GetReference()
    If wallRef != None
        wallRef.Disable()
    EndIf
    wallRef = Alias_FakeWall03.GetReference()
    If wallRef != None
        wallRef.Disable()
    EndIf
    wallRef = Alias_FakeWall04.GetReference()
    If wallRef != None
        wallRef.Disable()
    EndIf
    wallRef = Alias_FakeWall05.GetReference()
    If wallRef != None
        wallRef.Disable()
    EndIf
    wallRef = Alias_FakeBarrier01.GetReference()
    If wallRef != None
        wallRef.Disable()
    EndIf
    wallRef = Alias_FakeBarrier02.GetReference()
    If wallRef != None
        wallRef.Disable()
    EndIf
    wallRef = Alias_FakeBarrier03.GetReference()
    If wallRef != None
        wallRef.Disable()
    EndIf
    wallRef = Alias_FakeBarrier04.GetReference()
    If wallRef != None
        wallRef.Disable()
    EndIf
    wallRef = Alias_FakeBarrier05.GetReference()
    If wallRef != None
        wallRef.Disable()
    EndIf
    If GetStage() < 700
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    If GetStage() < 800
        SetStage(800)
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && playerRef.GetItemCount(W05_MQS_204P_IntelligenceModule) < 1
        playerRef.AddItem(W05_MQS_204P_IntelligenceModule, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.RemoveItem(W05_MQS_204P_IntelligenceModule, 1, True)
    EndIf
    If W05_MQS_204P_010_RepairDoneScene != None
        W05_MQS_204P_010_RepairDoneScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQ_204P_FactionChosen, 1.0)
    EndIf
    If W05_MQS_205P_QuestStartKeyword != None
        W05_MQS_205P_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction
