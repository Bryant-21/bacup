Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTarget)
    If iRequiredMenuID > 0 && auiMenuItemID != iRequiredMenuID
        Return
    EndIf
    FinishExam()
EndEvent

Function FinishExam()
    EN02_MQ_QuestScript examStore = Game.GetFormFromFile(0x000293A3, "SeventySix.esm") as EN02_MQ_QuestScript
    If examStore == None
        Return
    EndIf
    If Self == Game.GetFormFromFile(0x00001646, "SeventySix.esm") as Terminal
        examStore.CompleteExam()
    ElseIf Self == Game.GetFormFromFile(0x00068A09, "SeventySix.esm") as Terminal
        Quest motherlode = Game.GetFormFromFile(0x0006A379, "SeventySix.esm") as Quest
        If motherlode != None && motherlode.IsRunning() && !motherlode.IsCompleted() && !motherlode.IsStageDone(70) && examStore.GetStoredExamScore("MTR05") >= 5
            motherlode.SetStage(70)
        EndIf
    ElseIf Self == Game.GetFormFromFile(0x00073755, "SeventySix.esm") as Terminal || Self == Game.GetFormFromFile(0x00077F7B, "SeventySix.esm") as Terminal
        MTR06_QuestScript fireBreathers = Game.GetFormFromFile(0x0003363B, "SeventySix.esm") as MTR06_QuestScript
        If fireBreathers == None || !fireBreathers.IsRunning() || fireBreathers.IsCompleted() || fireBreathers.IsStageDone(20)
            Return
        EndIf
        Float passingScore = 7.0
        If fireBreathers.MTR06_ExamCorrectAnswersRequired != None
            passingScore = fireBreathers.MTR06_ExamCorrectAnswersRequired.GetValue()
        EndIf
        If examStore.GetStoredExamScore("MTR06") as Float >= passingScore
            If fireBreathers.PlayerCompletedKnowledgeExam != None
                fireBreathers.PlayerCompletedKnowledgeExam.ForceRefTo(Game.GetPlayer())
            EndIf
            fireBreathers.SetStage(20)
        EndIf
    EndIf
EndFunction
