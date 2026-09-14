Event OnQuestInit()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
        Alias_Player.ForceRefIfEmpty(playerRef)
    EndIf

    If QuestToStart != None && (QuestToStart.IsRunning() || QuestToStart.IsCompleted())
        Stop()
        Return
    EndIf

    If QuestStartKeyword != None && QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        Stop()
    Else
        StartTimer(5.0, 1)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != 1
        Return
    EndIf

    Actor playerRef = Alias_Player.GetActorReference()
    If QuestToStart != None && (QuestToStart.IsRunning() || QuestToStart.IsCompleted())
        Stop()
    ElseIf QuestStartKeyword != None && QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        Stop()
    Else
        StartTimer(5.0, 1)
    EndIf
EndEvent
