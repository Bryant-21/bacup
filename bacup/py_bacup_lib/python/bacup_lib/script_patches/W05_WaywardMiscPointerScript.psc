Function EvaluateMiscPointer()
    Actor player = Game.GetPlayer()
    If player == None
        Return
    EndIf

    If CharGen_ReclamationDay == None || !CharGen_ReclamationDay.IsCompleted()
        Return
    EndIf

    Int i = 0
    Int count = CompletedQuests.Length
    While i < count
        If CompletedQuests[i] != None && CompletedQuests[i].IsCompleted()
            Return
        EndIf
        i += 1
    EndWhile

    i = 0
    count = BlockingKeywords.Length
    While i < count
        If BlockingKeywords[i] != None && player.HasKeyword(BlockingKeywords[i])
            Return
        EndIf
        i += 1
    EndWhile

    player.AddKeyword(W05_Wayward_MiscPointer_QuestStartKeyword)
    player.SetValue(W05_MQ_001P_Wayward_PlayerStartedMiscPointer, 1.0)
EndFunction
