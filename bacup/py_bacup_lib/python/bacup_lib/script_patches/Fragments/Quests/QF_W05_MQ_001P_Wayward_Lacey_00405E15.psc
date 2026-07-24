Function Fragment_Stage_0010_Item_00()
    If Alias_owningPlayer
        Alias_owningPlayer.ForceRefIfEmpty(Game.GetPlayer())
    EndIf
EndFunction

Function Fragment_Stage_0015_Item_00()
    If W05_MQ_001P_Wayward != None && W05_MQ_001P_Wayward_QuestStartKeyword != None
        If !W05_MQ_001P_Wayward.IsRunning() && !W05_MQ_001P_Wayward.IsCompleted()
            ObjectReference playerRef = Game.GetPlayer()
            W05_MQ_001P_Wayward_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    If W05_MQ_001P_Wayward
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
EndFunction

Function Fragment_Stage_0200_Item_00()
EndFunction
