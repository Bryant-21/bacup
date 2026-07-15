Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTarget)
    If iRequiredMenuID > 0 && auiMenuItemID != iRequiredMenuID
        Return
    EndIf
    FinishExam()
EndEvent

Function FinishExam()
    Quest mainQuest = Game.GetFormFromFile(0x000293A3, "SeventySix.esm") as Quest
    If mainQuest == None
        Return
    EndIf
    EN02_MQ_QuestScript mainScript = mainQuest as EN02_MQ_QuestScript
    If mainScript != None
        mainScript.CompleteExam()
    ElseIf !mainQuest.IsStageDone(170)
        mainQuest.SetStage(170)
    EndIf
EndFunction
