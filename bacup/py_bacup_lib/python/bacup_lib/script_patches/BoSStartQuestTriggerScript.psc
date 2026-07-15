Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || QuestToStart == None
        Return
    EndIf
    If QuestToStart.IsRunning() || QuestToStart.IsCompleted()
        Return
    EndIf

    If QuestKeyword != None
        QuestKeyword.SendStoryEvent(None, akActionRef)
    Else
        QuestToStart.Start()
    EndIf
EndEvent
