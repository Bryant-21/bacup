; TODO

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_Choice_QuestComplete, 1.0)
    EndIf
    If W05_MQS_204P_QuestStartKeyword != None
        W05_MQS_204P_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction
