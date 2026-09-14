Event OnQuestInit()
    CancelTimer(iRaceCountdownTimerID)
    fRaceStartTime = 0.0
    fFinalRaceTime = 0.0
    StartTimer(iRaceCountdownTimerLength as Float, iRaceCountdownTimerID)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == iRaceCountdownTimerID && IsRunning() && IsStageDone(10) && !IsStageDone(iRaceStartStage)
        SetStage(iRaceStartStage)
    EndIf
EndEvent

Function MTR06_BeginRace()
    fRaceStartTime = Utility.GetCurrentRealTime()
    fFinalRaceTime = 0.0
EndFunction

Float Function MTR06_GetTrialLength()
    PhysicalExamTerminalRefScript terminalController = ActivatingTerminal.GetReference() as PhysicalExamTerminalRefScript
    If terminalController != None && terminalController.fTrialLength > 0.0
        Return terminalController.fTrialLength
    EndIf
    Return 0.0
EndFunction

Function MTR06_WrapUpRace()
    Float trialLength = MTR06_GetTrialLength()
    If fRaceStartTime <= 0.0 || trialLength <= 0.0
        fFinalRaceTime = 0.0
        SetStage(120)
        Return
    EndIf

    fFinalRaceTime = Utility.GetCurrentRealTime() - fRaceStartTime
    If fFinalRaceTime >= MinNumSeconds && fFinalRaceTime <= trialLength
        SetStage(110)
    Else
        SetStage(120)
    EndIf
EndFunction

Function MTR06_CompletePhysicalExam()
    Actor playerRef = ActivePlayer.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None && MTR06_PhysExamCompleted != None
        playerRef.SetValue(MTR06_PhysExamCompleted, 1.0)
    EndIf

    MTR06_QuestScript mainController = Game.GetFormFromFile(0x0003363B, "SeventySix.esm") as MTR06_QuestScript
    If mainController != None
        mainController.MTR06_HandlePhysicalExamComplete()
    EndIf
EndFunction

Function MTR06_ShowSuccess()
    If fFinalRaceTime > 0.0 && MTR06_PhysicalExamCompleteMessage != None
        MTR06_PhysicalExamCompleteMessage.Show(fFinalRaceTime)
    ElseIf MTR06_PhysicalExamCompleteMessage_Zeroed != None
        MTR06_PhysicalExamCompleteMessage_Zeroed.Show()
    EndIf
EndFunction

Function MTR06_ShowFailure()
    If fFinalRaceTime > 0.0 && MTR06_PhysicalExamFailedMessage != None
        MTR06_PhysicalExamFailedMessage.Show(fFinalRaceTime)
    ElseIf MTR06_PhysicalExamFailedMessage_Zeroed != None
        MTR06_PhysicalExamFailedMessage_Zeroed.Show()
    EndIf
EndFunction

Function MTR06_EndAttempt()
    CancelTimer(iRaceCountdownTimerID)
    Stop()
EndFunction

Event OnQuestShutdown()
    CancelTimer(iRaceCountdownTimerID)
    fRaceStartTime = 0.0
    fFinalRaceTime = 0.0
EndEvent
