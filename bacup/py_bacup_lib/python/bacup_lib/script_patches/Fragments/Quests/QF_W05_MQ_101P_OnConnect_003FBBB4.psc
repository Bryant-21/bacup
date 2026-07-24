Function Fragment_Stage_0010_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef != None && W05_MQ_101P_QuestStartKeyword != None
        W05_MQ_101P_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
    EndIf
    Stop()
EndFunction
