Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Game.GetPlayer()
    If W05_MQ_102P_A != None
        W05_MQ_102P_A.Stop()
    EndIf
    If W05_MQR_201P != None
        W05_MQR_201P.Stop()
    EndIf
    If W05_MQR_202P != None
        W05_MQR_202P.Stop()
    EndIf
    If W05_MQR_203P != None
        W05_MQR_203P.Stop()
    EndIf
    If W05_MQR_Choice != None
        W05_MQR_Choice.Stop()
    EndIf
    If playerRef != None
        playerRef.SetValue(W05_MQS_Choice_QuestComplete, 1.0)
    EndIf
    If W05_MQS_204P_QuestStartKeyword != None
        W05_MQS_204P_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction

Function Fragment_Stage_9999_Item_00()
    SetObjectiveFailed(100)
EndFunction
