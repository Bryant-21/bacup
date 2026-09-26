Event OnQuestInit()
    Num_EmotesDone = 0
    Quest owner = Self as Quest
    eventQuestScript = owner as DefaultEventQuest
    PublishEmoteCount()
EndEvent

Event OnQuestShutdown()
    CancelTimer(Round2IntroTimerID)
    CancelTimer(Round3IntroTimerID)
    CancelTimer(FirstBonusObjAnnounceTimerID)
    CancelTimer(SecondBonusObjAnnounceTimerID)
    CancelTimer(FailSafeTimerID)
EndEvent

Bool Function EventResolved()
    Return IsStageDone(9000) || IsStageDone(9991) || IsStageDone(9992) || IsStageDone(Stage_Failsafe)
EndFunction

Function ScheduleRoundIntro(Int aiRound)
    If aiRound == 2
        CancelTimer(Round2IntroTimerID)
        StartTimer(15.0, Round2IntroTimerID)
    ElseIf aiRound == 3
        CancelTimer(Round3IntroTimerID)
        StartTimer(15.0, Round3IntroTimerID)
    EndIf
EndFunction

Function AnnounceGoldenEyebotBonus()
    CancelTimer(FirstBonusObjAnnounceTimerID)
    StartTimer(3.0, FirstBonusObjAnnounceTimerID)
EndFunction

Function AnnounceEmoteBonus()
    CancelTimer(SecondBonusObjAnnounceTimerID)
    StartTimer(3.0, SecondBonusObjAnnounceTimerID)
EndFunction

Function ArmFailsafe(Float afDelay)
    CancelTimer(FailSafeTimerID)
    StartTimer(afDelay, FailSafeTimerID)
EndFunction

Bool Function IsBonusObjectiveOpen(Int aiObjective)
    Return IsRunning() && !EventResolved() && IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
EndFunction

Function PublishEmoteCount()
    Quest owner = Self as Quest
    B21:QuestVariables questVariables = owner as B21:QuestVariables
    If questVariables != None
        questVariables.SetVariable("Emotes_Done", Num_EmotesDone as Float)
    EndIf
EndFunction

Bool Function RegisterEmote()
    If Stage_EmotesComplete < 0 || IsStageDone(Stage_EmotesComplete) || !IsBonusObjectiveOpen(65)
        Return False
    EndIf
    Num_EmotesDone += 1
    PublishEmoteCount()
    If Num_EmotesDone >= Goal_EmotesDone
        SetStage(Stage_EmotesComplete)
    EndIf
    Return True
EndFunction

Function StartRoundScene(Scene akScene, Int aiRoundStartStage)
    If akScene == None || !IsRunning() || EventResolved() || IsStageDone(aiRoundStartStage)
        Return
    EndIf
    If !akScene.IsPlaying()
        akScene.Start()
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == Round2IntroTimerID
        StartRoundScene(Round2StartScene, 500)
    ElseIf aiTimerID == Round3IntroTimerID
        StartRoundScene(Round3StartScene, 700)
    ElseIf aiTimerID == FirstBonusObjAnnounceTimerID
        If FirstBonusObjMessage != None && IsBonusObjectiveOpen(55)
            FirstBonusObjMessage.Show()
        EndIf
    ElseIf aiTimerID == SecondBonusObjAnnounceTimerID
        If SecondBonusObjMessage != None && IsBonusObjectiveOpen(65)
            SecondBonusObjMessage.Show()
        EndIf
    ElseIf aiTimerID == FailSafeTimerID
        If !IsRunning()
            Return
        EndIf
        If EventResolved()
            Stop()
        ElseIf IsStageDone(800)
            ; Pappas' closing topic sets 9000 when it finishes; an unloaded speaker never finishes it.
            SetStage(9000)
        ElseIf IsStageDone(900)
            SetStage(9992)
        Else
            SetStage(Stage_Failsafe)
        EndIf
    EndIf
EndEvent
