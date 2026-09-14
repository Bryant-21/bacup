Function Fragment_Stage_0015_Item_00()
    DispatchWaywardStartEvent()
EndFunction

Function DispatchWaywardStartEvent()
    If W05_MQ_001P_Wayward != None && W05_MQ_001P_Wayward_QuestStartKeyword != None
        If !W05_MQ_001P_Wayward.IsRunning() && !W05_MQ_001P_Wayward.IsCompleted()
            ObjectReference playerRef = Game.GetPlayer()
            W05_MQ_001P_Wayward_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
            Debug.Trace("[B21] Wayward start event sent; running=" + W05_MQ_001P_Wayward.IsRunning())
        EndIf
    EndIf
EndFunction

Function SetLaceyIselaCheckpoint(float checkpointValue)
    ObjectReference playerRef
    If Alias_owningPlayer
        playerRef = Alias_owningPlayer.GetReference()
    EndIf
    If !playerRef
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef && W05_MQ_001P_Wayward_LaceyIsela_Checkpoint
        playerRef.SetValue(W05_MQ_001P_Wayward_LaceyIsela_Checkpoint, checkpointValue)
    EndIf
EndFunction

; Wayward is event scoped: only Story Manager can start it, so every SetStage on
; it from this quest's dialogue is refused until the start keyword has been sent.
; Stage 15 ("try and throw W05_Wayward") is never reached in FO4 — FO76 drove it
; server-side. Stage 10 is RunOnStart but carries NO VMAD fragment entry, so a
; Fragment_Stage_0010_Item_00 added here is never called and is pruned from the
; build. Stage 30 DOES have a fragment entry and fires when the player triggers a
; main-scene greeting, which precedes the topic info that sets Wayward's stage.
Function Fragment_Stage_0030_Item_00()
    DispatchWaywardStartEvent()
    SetLaceyIselaCheckpoint(1.0)
    If W05_MQ_001P_Wayward_LaceyIselaScene_010
        W05_MQ_001P_Wayward_LaceyIselaScene_010.Stop()
    EndIf
    If W05_MQ_001P_Wayward_LaceyIselaScene_020
        W05_MQ_001P_Wayward_LaceyIselaScene_020.Stop()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    If W05_MQ_001P_Wayward && W05_MQ_001P_Wayward.IsRunning()
        If !W05_MQ_001P_Wayward.IsStageDone(200)
            W05_MQ_001P_Wayward.SetStage(200)
        EndIf
    EndIf
    ObjectReference playerRef
    If Alias_owningPlayer
        playerRef = Alias_owningPlayer.GetReference()
    EndIf
    If playerRef
        playerRef.SetValue(W05_MQ_001P_Wayward_LaceyIsela_PlayerTriggeredLaceyIselaMainConvo, 1.0)
    EndIf
    SetLaceyIselaCheckpoint(10.0)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetLaceyIselaCheckpoint(10.0)
    Stop()
EndFunction
