; RE_SceneSM04 insult bot. The quest fills TargetedPlayer/InsultedPlayer and sets the placeholder stage 15 so the
; bot's DefaultAliasOnDistanceLessThan (300 units) reports GreetStage 100 when RE_SceneSM04_GotoPlayer gets it
; there. The insult note (RewardNote) follows (200); an unfinished pursuit gives up after GiveUpTimerLength (300).
Event OnQuestInit()
    Actor player = Game.GetPlayer()
    If TargetedPlayer.GetReference() == None
        TargetedPlayer.ForceRefTo(player)
    EndIf
    If InsultedPlayer.GetReference() == None
        InsultedPlayer.ForceRefTo(player)
    EndIf
    SetStage(15)
    StartTimer(GiveUpTimerLength as Float, GiveUpTimerID)
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == GreetStage
        StartTimer(5.0, 11)
    ElseIf auiStageID == 200
        CancelTimer(GiveUpTimerID)
        ObjectReference target = TargetedPlayer.GetReference()
        If target != None && RewardNote != None
            target.AddItem(RewardNote, 1)
        EndIf
    ElseIf auiStageID == 300
        If GiveUpScene != None && !GiveUpScene.IsPlaying()
            GiveUpScene.Start()
        EndIf
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If !IsRunning()
        Return
    EndIf
    If aiTimerID == 11
        If !IsStageDone(200) && !IsStageDone(250)
            SetStage(200)
        EndIf
    ElseIf aiTimerID == GiveUpTimerID
        If !IsStageDone(GreetStage) && !IsStageDone(250)
            SetStage(300)
        EndIf
    EndIf
EndEvent
