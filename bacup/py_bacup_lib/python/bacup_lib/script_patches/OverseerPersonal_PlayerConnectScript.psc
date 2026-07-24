Event OnQuestInit()
    If OverseerPersonal_QuestStartKeyword != None
        OverseerPersonal_QuestStartKeyword.SendStoryEventAndWait()
    EndIf
    Stop()
EndEvent
