Function ArmStageTimer(Int aiStage, Float afSeconds)
    If aiStage <= 0 || IsStageDone(aiStage)
        Return
    EndIf
    Float delay = afSeconds
    If delay < 1.0
        delay = 1.0
    EndIf
    CancelTimer(aiStage)
    StartTimer(delay, aiStage)
EndFunction

Function ArmTalkFailsafe()
    CancelTimer(9200)
    StartTimer(60.0, 9200)
EndFunction

Function ArmWaveStop(Int aiWaveIndex, Float afSeconds)
    If aiWaveIndex < 0 || aiWaveIndex > 99
        Return
    EndIf
    Float delay = afSeconds
    If delay < 1.0
        delay = 1.0
    EndIf
    CancelTimer(9100 + aiWaveIndex)
    StartTimer(delay, 9100 + aiWaveIndex)
EndFunction

Event OnQuestInit()
    Quest owner = Self as Quest
    B21:QuestTimer questTimer = owner as B21:QuestTimer
    If questTimer != None
        RegisterForCustomEvent(questTimer, "QuestTimerEnded")
    EndIf
    If SFZ08_Fear_BeckhamDialogue != None && !SFZ08_Fear_BeckhamDialogue.IsRunning()
        SFZ08_Fear_BeckhamDialogue.Start()
    EndIf
EndEvent

Event B21:QuestTimer.QuestTimerEnded(B21:QuestTimer akSender, Var[] akArgs)
    If !IsRunning() || IsStageDone(400) || IsStageDone(500) || IsStageDone(1100) || IsStageDone(1110)
        Return
    EndIf
    SetStage(1110)
EndEvent

Event OnTimer(Int aiTimerID)
    If !IsRunning()
        Return
    EndIf

    If aiTimerID == 9200
        ; Standing in for FO76's "activate Beckham" start when his converted
        ; dialogue cannot be reached at all.
        If IsStageDone(50)
            Return
        EndIf
        Actor beckhamActor = None
        If Beckham != None
            beckhamActor = Beckham.GetActorReference()
        EndIf
        Actor playerRef = Game.GetPlayer()
        If beckhamActor != None && playerRef != None && beckhamActor.GetDistance(playerRef) <= 512.0
            SetStage(50)
            Return
        EndIf
        StartTimer(15.0, 9200)
        Return
    EndIf
    If aiTimerID >= 9100 && aiTimerID <= 9199
        Quest owner = Self as Quest
        DefaultQuestEncounterWaveScript waveScript = owner as DefaultQuestEncounterWaveScript
        If waveScript != None
            waveScript.StopEncounterWave(aiTimerID - 9100, False)
        EndIf
        Return
    EndIf
    If aiTimerID > 0 && aiTimerID <= 2000 && !IsStageDone(aiTimerID)
        SetStage(aiTimerID)
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(9100)
    CancelTimer(9101)
    CancelTimer(9102)
    CancelTimer(9200)
    UnregisterForAllEvents()
EndEvent
