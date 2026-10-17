; RE_ScenePS03 sweepstakes Handy. RE_ScenePS03_GotoPlayer walks the robot to PrizePlayer while the stage is below
; RobotGreetStage. Reaching the player greets (100) and the prize (LL_Reward_RE_ScenePS03) follows (200). An
; undelivered prize gives up after the timeout (300, RE_ScenePS03_GiveUpScene).
Event OnQuestInit()
    Actor player = Game.GetPlayer()
    If PrizePlayer.GetReference() == None
        PrizePlayer.ForceRefTo(player)
    EndIf
    If PrizeWinner.GetReference() == None
        PrizeWinner.ForceRefTo(player)
    EndIf
    StartTimer(2.0, 10)
    StartTimer(900.0, GiveUpTimerID)
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == RobotGreetStage
        StartTimer(5.0, 11)
    ElseIf auiStageID == 200
        CancelTimer(GiveUpTimerID)
        ObjectReference winner = PrizePlayer.GetReference()
        If winner != None && LL_Reward_RE_ScenePS03 != None
            winner.AddItem(LL_Reward_RE_ScenePS03, 1)
        EndIf
    ElseIf auiStageID == 300
        If RE_ScenePS03_GiveUpScene != None && !RE_ScenePS03_GiveUpScene.IsPlaying()
            RE_ScenePS03_GiveUpScene.Start()
        EndIf
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If !IsRunning()
        Return
    EndIf
    If aiTimerID == 10
        If IsStageDone(RobotGreetStage) || IsStageDone(250) || IsStageDone(300)
            Return
        EndIf
        ObjectReference robot = Handy.GetReference()
        ObjectReference winner = PrizePlayer.GetReference()
        If robot != None && winner != None && robot.GetDistance(winner) <= 600.0
            SetStage(RobotGreetStage)
            Return
        EndIf
        StartTimer(2.0, 10)
    ElseIf aiTimerID == 11
        If !IsStageDone(200) && !IsStageDone(250)
            SetStage(200)
        EndIf
    ElseIf aiTimerID == GiveUpTimerID
        If !IsStageDone(200) && !IsStageDone(250)
            SetStage(300)
        EndIf
    EndIf
EndEvent
