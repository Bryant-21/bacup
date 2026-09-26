Bool Function SendQuestStoryEvent()
    Actor playerRef = Game.GetPlayer()
    If !IsRunning() || playerRef == None || MyStoryManagerKeyword == None
        Return False
    EndIf
    If QuestActiveKeyword != None && playerRef.HasKeyword(QuestActiveKeyword)
        Return True
    EndIf
    If QuestCompletedActorValue != None && playerRef.GetValue(QuestCompletedActorValue) > QuestCompletedMaxCount
        Return True
    EndIf
    ObjectReference firstRef = akRef1ToSend
    ObjectReference playerToAdd = None
    If AddPlayerToQuest
        playerToAdd = playerRef
        If firstRef == None
            firstRef = playerRef
        EndIf
    EndIf
    Return MyStoryManagerKeyword.SendStoryEventAndWait(akLoc, firstRef, playerToAdd, iValue1, iValue2)
EndFunction
