EN05_CombatCourseScript Function EN05CBTF_GetCourse()
    Return (Self as Quest) as EN05_CombatCourseScript
EndFunction

Function EN05CBTF_Announce(Topic akTopic)
    If akTopic == None || Alias_Loudspeaker == None
        Return
    EndIf
    ObjectReference speaker = Alias_Loudspeaker.GetRef()
    If speaker != None
        speaker.Say(akTopic)
    EndIf
EndFunction

; The wave data lives on the sibling DefaultQuestEncounterWaveScript bound to
; this same quest; each wave is started explicitly from its stage.
Function EN05CBTF_StartWave(Int aiWaveIndex)
    Int startStage = 20
    Int stopStage = 30
    If aiWaveIndex == 1
        startStage = 32
        stopStage = 40
    ElseIf aiWaveIndex == 2
        startStage = 42
        stopStage = 100
    EndIf
    If !IsRunning() || !IsStageDone(startStage) || GetStage() >= stopStage
        CancelTimer(8760 + aiWaveIndex)
        Return
    EndIf
    B21:LocalEncounterMaterializer materializer = (Self as Quest) as B21:LocalEncounterMaterializer
    If materializer != None
        materializer.PrepareEligibleWaves()
    EndIf
    If materializer == None || !materializer.HasPreparedWave(aiWaveIndex)
        StartTimer(5.0, 8760 + aiWaveIndex)
        Return
    EndIf
    If aiWaveIndex == 2 && !materializer.HasPreparedWave(3)
        StartTimer(5.0, 8760 + aiWaveIndex)
        Return
    EndIf
    RefCollectionAlias enemies = GetAlias(6) as RefCollectionAlias
    RefCollectionAlias boss = GetAlias(13) as RefCollectionAlias
    DefaultQuestEncounterWaveScript waves = (Self as Quest) as DefaultQuestEncounterWaveScript
    If enemies == None || enemies.GetCount() == 0 || waves == None
        StartTimer(5.0, 8760 + aiWaveIndex)
        Return
    EndIf
    If aiWaveIndex == 2 && (boss == None || boss.GetCount() == 0)
        StartTimer(5.0, 8760 + aiWaveIndex)
        Return
    EndIf
    waves.StartLocalEncounterWave(aiWaveIndex)
    CancelTimer(8760 + aiWaveIndex)
EndFunction

Event OnQuestInit()
    RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    Int stageID = GetStage()
    If stageID >= 42 && stageID < 100
        EN05CBTF_StartWave(2)
    ElseIf stageID >= 32 && stageID < 40
        EN05CBTF_StartWave(1)
    ElseIf stageID >= 20 && stageID < 30
        EN05CBTF_StartWave(0)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID >= 8760 && aiTimerID <= 8762
        EN05CBTF_StartWave(aiTimerID - 8760)
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(8760)
    CancelTimer(8761)
    CancelTimer(8762)
    UnregisterForAllRemoteEvents()
EndEvent

Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10)
    If EN05_CombatCourse_Intro != None
        EN05_CombatCourse_Intro.Start()
    EndIf
    EN05_CombatCourseScript course = EN05CBTF_GetCourse()
    If course != None
        course.EN05Course_BeginCountdown()
    EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)

    EN05_CombatCourseScript course = EN05CBTF_GetCourse()
    If course != None
        course.EN05Course_StartCourse()
    EndIf

    EN05CBTF_Announce(EN05_CBT_FirstWave)
    EN05CBTF_StartWave(0)
EndFunction

Function Fragment_Stage_0030_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)
    EN05CBTF_Announce(EN05_CBT_Cooldown)

    EN05_CombatCourseScript course = EN05CBTF_GetCourse()
    If course != None
        course.EN05CBT_StartWaveCooldown(course.iWave02StartStage)
    EndIf
EndFunction

Function Fragment_Stage_0032_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(32)
    EN05CBTF_Announce(EN05_CBT_SecondWave)
    EN05CBTF_StartWave(1)
EndFunction

Function Fragment_Stage_0040_Item_00()
    SetObjectiveCompleted(32)
    SetObjectiveDisplayed(40)
    EN05CBTF_Announce(EN05_CBT_CooldownTwo)

    EN05_CombatCourseScript course = EN05CBTF_GetCourse()
    If course != None
        course.EN05CBT_StartWaveCooldown(course.iWave03StartStage)
    EndIf
EndFunction

Function Fragment_Stage_0042_Item_00()
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(42)
    EN05CBTF_Announce(EN05_CBT_FinalWave)
    EN05CBTF_StartWave(2)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(42)

    EN05_CombatCourseScript course = EN05CBTF_GetCourse()
    If course != None
        course.EN05Course_FinishCourse()
        course.EN05Course_WrapUp(True)
    EndIf
EndFunction

Function Fragment_Stage_0110_Item_00()
    CompleteAllObjectives()

    EN05_CombatCourseScript course = EN05CBTF_GetCourse()
    If course != None
        course.EN05CBT_MarkCompleted()
        course.EN05CBT_AnnounceComplete()
        course.EN05Course_ShowResultMessage(True)
        course.EN05Course_ReportCompletion()
    EndIf

    If EN05_CBT_Wrapup != None && Alias_MasterSgt != None
        ObjectReference sergeant = Alias_MasterSgt.GetRef()
        If sergeant != None
            sergeant.Say(EN05_CBT_Wrapup)
        EndIf
    EndIf

    If !IsStageDone(150)
        SetStage(150)
    EndIf
EndFunction

Function Fragment_Stage_0120_Item_00()
    FailAllObjectives()

    EN05_CombatCourseScript course = EN05CBTF_GetCourse()
    If course != None
        course.EN05Course_ShowResultMessage(False)
    EndIf

    If !IsStageDone(150)
        SetStage(150)
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    Stop()
EndFunction
