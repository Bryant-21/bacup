Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_8000_Item_00()
    If Alias_currentPlayer
        ObjectReference currentPlayer = Alias_currentPlayer.GetReference()
        If currentPlayer && currentPlayer.GetValue(W05_MQ_102P_RepRewardGranted) == 0.0
            currentPlayer.ModValue(Reputation_AV_Foundation, Rep_Mod_MQ_Add_Small.GetValue())
            currentPlayer.SetValue(W05_MQ_102P_RepRewardGranted, 1.0)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    W05_MQS_201P_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    If W05_MQ_102P && !W05_MQ_102P.IsStageDone(1700)
        W05_MQ_102P.SetStage(1700)
    EndIf
EndFunction
