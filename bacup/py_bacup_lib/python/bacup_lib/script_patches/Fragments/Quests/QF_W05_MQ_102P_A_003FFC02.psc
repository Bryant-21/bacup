Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0300_Item_00()
    If W05_MQ_102P_A_MegIntroScene02 && !W05_MQ_102P_A_MegIntroScene02.IsPlaying()
        W05_MQ_102P_A_MegIntroScene02.Start()
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    If Alias_currentPlayer
        ObjectReference currentPlayer = Alias_currentPlayer.GetReference()
        If currentPlayer && currentPlayer.GetValue(W05_MQ_102P_RepRewardGranted) == 0.0
            currentPlayer.ModValue(Reputation_AV_Crater, Rep_Mod_MQ_Add_Small.GetValue())
            currentPlayer.SetValue(W05_MQ_102P_RepRewardGranted, 1.0)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_9500_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    W05_MQR_201P_QuestStart_Keyword.SendStoryEvent(None, playerRef, playerRef)
    If W05_MQ_102P && !W05_MQ_102P.IsStageDone(1600)
        W05_MQ_102P.SetStage(1600)
    EndIf
    Stop()
EndFunction
