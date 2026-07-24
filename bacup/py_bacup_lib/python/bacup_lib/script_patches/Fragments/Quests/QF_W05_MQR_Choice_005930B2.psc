; TODO

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0200_Item_00()
    If W05_MQR_204P_WarningMSG != None
        W05_MQR_204P_WarningMSG.Show()
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Game.GetPlayer()
    If W05_MQ_102P_B != None
        W05_MQ_102P_B.Stop()
    EndIf
    If W05_MQS_201P != None
        W05_MQS_201P.Stop()
    EndIf
    If W05_MQS_202P != None
        W05_MQS_202P.Stop()
    EndIf
    If W05_MQS_203P != None
        W05_MQS_203P.Stop()
    EndIf
    If W05_MQS_Choice != None
        W05_MQS_Choice.Stop()
    EndIf
    If playerRef != None
        playerRef.SetValue(W05_MQR_Choice_QuestComplete, 1.0)
    EndIf
    If W05_MQR_204P_QuestStart_Keyword != None
        W05_MQR_204P_QuestStart_Keyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction
