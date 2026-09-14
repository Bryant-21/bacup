Function Fragment_Stage_0010_Item_00()
    Actor playerRef = Alias_currentPlayer.GetReference() as Actor
    If playerRef != None && W05_MQ_101P_QuestStartKeyword != None
        W05_MQ_101P_QuestStartKeyword.SendStoryEventAndWait(None, playerRef)
    EndIf
    Stop()
EndFunction
