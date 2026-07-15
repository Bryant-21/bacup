Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTarget)
    If iRequiredMenuID > 0 && auiMenuItemID != iRequiredMenuID
        Return
    EndIf
    EN02_ExamPlayerScript playerScript = Game.GetPlayer() as EN02_ExamPlayerScript
    If playerScript == None
        Return
    EndIf
    If bClearSubterminalArray
        playerScript.ResetAnswers()
    EndIf
    Int responseValue = ResolveAnswerValue(auiMenuItemID)
    playerScript.RecordAnswer(Self, responseValue)
EndEvent

Int Function ResolveAnswerValue(Int auiMenuItemID)
    Int i = 0
    While i < TargetAnswers.Length
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
