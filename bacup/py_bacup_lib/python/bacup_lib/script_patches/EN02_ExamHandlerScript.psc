Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        RegisterExamResults()
        UpdateExamScore()
    EndIf
EndEvent

Event OnLoad()
    RegisterExamResults()
EndEvent

Event OnUnload()
    Terminal results = GetExamResultsTerminal()
    If results != None
        UnregisterForRemoteEvent(results, "OnMenuItemRun")
    EndIf
EndEvent

Function RegisterExamResults()
    Terminal results = GetExamResultsTerminal()
    If results != None
        RegisterForRemoteEvent(results, "OnMenuItemRun")
    EndIf
EndFunction

Terminal Function GetExamResultsTerminal()
    If ExamScoreTrackingValue == Game.GetFormFromFile(0x00439187, "SeventySix.esm") as ActorValue
        Return Game.GetFormFromFile(0x00068AD4, "SeventySix.esm") as Terminal
    ElseIf ExamScoreTrackingValue == Game.GetFormFromFile(0x00439185, "SeventySix.esm") as ActorValue
        Return Game.GetFormFromFile(0x00073733, "SeventySix.esm") as Terminal
    EndIf
    Return None
EndFunction

Event Terminal.OnMenuItemRun(Terminal akSender, Int auiMenuItemID, ObjectReference akTerminalRef)
    If akTerminalRef != Self || auiMenuItemID != 1 || akSender != GetExamResultsTerminal()
        Return
    EndIf
    EN02_ExamWrapupScript wrapup = None
    If ExamScoreTrackingValue == Game.GetFormFromFile(0x00439187, "SeventySix.esm") as ActorValue
        wrapup = Game.GetFormFromFile(0x00068A09, "SeventySix.esm") as EN02_ExamWrapupScript
    ElseIf ExamScoreTrackingValue == Game.GetFormFromFile(0x00439185, "SeventySix.esm") as ActorValue
        wrapup = Game.GetFormFromFile(0x00073755, "SeventySix.esm") as EN02_ExamWrapupScript
    EndIf
    If wrapup != None
        wrapup.FinishExam()
    EndIf
EndEvent

Function UpdateExamScore()
    If bUpdateLock || ExamScoreTrackingValue == None
        Return
    EndIf
    bUpdateLock = True
    EN02_MQ_QuestScript examStore = Game.GetFormFromFile(0x000293A3, "SeventySix.esm") as EN02_MQ_QuestScript
    If examStore != None
        String channel = ""
        If ExamScoreTrackingValue == Game.GetFormFromFile(0x00439187, "SeventySix.esm") as ActorValue
            channel = "MTR05"
        ElseIf ExamScoreTrackingValue == Game.GetFormFromFile(0x00439185, "SeventySix.esm") as ActorValue
            channel = "MTR06"
        ElseIf ExamScoreTrackingValue == Game.GetFormFromFile(0x00439186, "SeventySix.esm") as ActorValue
            channel = "EN02"
        EndIf
        If channel != ""
            Game.GetPlayer().SetValue(ExamScoreTrackingValue, examStore.GetStoredExamScore(channel) as Float)
        EndIf
    EndIf
    StartTimer(0.1, 0)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 0
        bUpdateLock = False
    EndIf
EndEvent
