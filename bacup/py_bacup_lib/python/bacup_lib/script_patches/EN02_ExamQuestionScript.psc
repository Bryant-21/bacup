Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTarget)
    If iRequiredMenuID > 0 && auiMenuItemID != iRequiredMenuID
        Return
    EndIf
    EN02_MQ_QuestScript examStore = Game.GetFormFromFile(0x000293A3, "SeventySix.esm") as EN02_MQ_QuestScript
    If examStore == None
        Return
    EndIf
    If bClearSubterminalArray
        examStore.ResetStoredExam(DejaChannel)
    Else
        Int responseValue = ResolveAnswerValue(auiMenuItemID)
        examStore.RecordStoredExamAnswer(Self, responseValue)
    EndIf
    ActorValue scoreValue = GetExamScoreValue()
    If scoreValue != None
        Game.GetPlayer().SetValue(scoreValue, examStore.GetStoredExamScore(DejaChannel) as Float)
    EndIf
EndEvent

ActorValue Function GetExamScoreValue()
    If DejaChannel == "EN02"
        Return Game.GetFormFromFile(0x00439186, "SeventySix.esm") as ActorValue
    ElseIf DejaChannel == "MTR05"
        Return Game.GetFormFromFile(0x00439187, "SeventySix.esm") as ActorValue
    ElseIf DejaChannel == "MTR06"
        Return Game.GetFormFromFile(0x00439185, "SeventySix.esm") as ActorValue
    EndIf
    Return None
EndFunction

Int Function ResolveAnswerValue(Int auiMenuItemID)
    Int i = 0
    While TargetAnswers != None && i < TargetAnswers.Length
        If TargetAnswers[i].iMenuID == auiMenuItemID
            Return TargetAnswers[i].iAnswerValue
        EndIf
        i = i + 1
    EndWhile
    If iSecondaryCorrectAnswer > 0 && auiMenuItemID == iSecondaryCorrectAnswer
        Return iAmount
    EndIf
    Return 0
EndFunction
