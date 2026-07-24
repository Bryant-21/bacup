Event OnQuestInit()
    If MQ_Overseer_QuestStartKeyword != None
        MQ_Overseer_QuestStartKeyword.SendStoryEventAndWait()
    EndIf
    Stop()
EndEvent
