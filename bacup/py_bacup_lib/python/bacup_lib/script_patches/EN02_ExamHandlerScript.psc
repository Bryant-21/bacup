Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        UpdateExamScore()
    EndIf
EndEvent

Function UpdateExamScore()
    If bUpdateLock || ExamScoreTrackingValue == None
        Return
    EndIf
    bUpdateLock = True
    EN02_ExamPlayerScript playerScript = Game.GetPlayer() as EN02_ExamPlayerScript
    If playerScript != None
        Game.GetPlayer().SetValue(ExamScoreTrackingValue, playerScript.iPlayerCorrectAnswers)
    EndIf
    StartTimer(0.1, 0)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 0
        bUpdateLock = False
    EndIf
EndEvent
