Actor Function EN05Basic_GetPlayer()
    Actor player = None
    If ActivePlayer != None
        player = ActivePlayer.GetActorReference()
    EndIf
    If player == None
        player = Game.GetPlayer()
    EndIf
    Return player
EndFunction

Function EN05Basic_FillSceneActors()
    ReferenceAlias sergeantObjective = GetAlias(5) as ReferenceAlias
    Actor sergeant = None
    If sergeantObjective != None
        sergeant = sergeantObjective.GetActorReference()
    EndIf
    If sergeant == None
        sergeant = Game.GetFormFromFile(0x001820C6, "SeventySix.esm") as Actor
    EndIf
    If sergeant != None
        If DrillSergeant != None && DrillSergeant.GetReference() != sergeant
            DrillSergeant.ForceRefTo(sergeant)
        EndIf
        If CourseCompletionSceneTarget != None && CourseCompletionSceneTarget.GetReference() == None
            CourseCompletionSceneTarget.ForceRefTo(sergeant)
        EndIf
    EndIf
EndFunction

Function EN05Basic_ProcessCourseCompletion()
    If !IsRunning() || IsCompleted() || iCoursesCompletedCount < 1 \
        || IsStageDone(iInitialCoursesCompletedStage) || IsStageDone(iCombatCompleteStage)
        Return
    EndIf
    EN05Basic_FillSceneActors()
    If EN05_Basic_0050_ProcessCourseCompletion != None && !EN05_Basic_0050_ProcessCourseCompletion.IsPlaying()
        EN05_Basic_0050_ProcessCourseCompletion.ForceStart()
    EndIf
EndFunction

Function EN05Basic_ReconcileOnLoad()
    If !IsRunning() || IsCompleted()
        Return
    EndIf
    EN05Basic_FillSceneActors()
    EN05Basic_ReconcileUniform()
    If IsStageDone(iMarksmanshipCompleteStage) && IsStageDone(iObstacleCompleteStage) && IsStageDone(iPatriotismCompleteStage)
        iCoursesCompletedCount = iCoursesRequired
        EN05Basic_ProcessCourseCompletion()
    EndIf
EndFunction

Function EN05Basic_ReconcileUniform()
    If !IsRunning() || IsCompleted()
        Return
    EndIf
    Actor player = EN05Basic_GetPlayer()
    If player == None
        Return
    EndIf
    bFatiguesEquipped = EN05_MilitaryFatigueKeyword != None && player.WornHasKeyword(EN05_MilitaryFatigueKeyword)
    bHatEquipped = EN05_MilitaryHelmetKeyword != None && player.WornHasKeyword(EN05_MilitaryHelmetKeyword)
    If !StartupStateSet
        bPlayerArrivedInUniform = bFatiguesEquipped && bHatEquipped
        StartupStateSet = True
    EndIf
    If IsStageDone(CollectUniformStage) && !IsStageDone(UniformCollectedStage)
        SetObjectiveCompleted(10, bFatiguesEquipped)
        SetObjectiveCompleted(11, bHatEquipped)
        SetObjectiveDisplayed(20, bFatiguesEquipped && bHatEquipped)
        If EN05_PlayerReadUniformLogValue != None && player.GetValue(EN05_PlayerReadUniformLogValue) > 0.0 && !IsStageDone(iReadUniformLogStage)
            SetStage(iReadUniformLogStage)
        EndIf
    EndIf
EndFunction

Function EN05Basic_BeginIntroduction(Bool abInUniform)
    If !IsRunning() || IsCompleted() || IsStageDone(UniformCollectedStage)
        Return
    EndIf
    If !IsStageDone(CollectUniformStage)
        bPlayerArrivedInUniform = abInUniform
        StartupStateSet = True
    EndIf
    bKickOffStartUpScene = False
    If !abInUniform && !IsStageDone(CollectUniformStage)
        SetStage(CollectUniformStage)
    EndIf
    EN05Basic_ReconcileUniform()
EndFunction

Event OnQuestInit()
    Actor player = EN05Basic_GetPlayer()
    If ActivePlayer != None && ActivePlayer.GetReference() == None && player != None
        ActivePlayer.ForceRefTo(player)
    EndIf
    EN05Basic_FillSceneActors()
    EN05Basic_ReconcileUniform()
    If !IsStageDone(CollectUniformStage) && !IsStageDone(UniformCollectedStage)
        bKickOffStartUpScene = True
        If EN05_Basic_0000_StartUpScene != None && !EN05_Basic_0000_StartUpScene.IsPlaying()
            EN05_Basic_0000_StartUpScene.Start()
        EndIf
    EndIf
    bQuestInited = True
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == CollectUniformStage || auiStageID == 15
        EN05Basic_ReconcileUniform()
    ElseIf auiStageID == UniformCollectedStage
        bKickOffStartUpScene = False
    EndIf
EndEvent

Function EN05Basic_CourseCompleted(Int aiCourseID)
    If !IsRunning()
        Return
    EndIf

    Int stageToSet = -1
    If aiCourseID == iMarkmanshipCourseID
        stageToSet = iMarksmanshipCompleteStage
    ElseIf aiCourseID == iObstacleCourseID
        stageToSet = iObstacleCompleteStage
    ElseIf aiCourseID == iPatriotismCourseID
        stageToSet = iPatriotismCompleteStage
    ElseIf aiCourseID == iCombatCourseID
        Actor player = EN05Basic_GetPlayer()
        If player != None && EN05_CBT_CompletedValue != None
            player.SetValue(EN05_CBT_CompletedValue, 1.0)
        EndIf
        If IsStageDone(iInitialCoursesCompletedStage)
            stageToSet = iCombatCompleteStage
        EndIf
    EndIf

    If stageToSet >= 0 && !IsStageDone(stageToSet)
        SetStage(stageToSet)
    EndIf
EndFunction
Function EN05Basic_ShowPowerArmorWarning()
    Actor player = EN05Basic_GetPlayer()
    If !IsRunning() || IsCompleted() || bInCooldown || player == None || !player.IsInPowerArmor()
        Return
    EndIf
    If EN05_Basic_NoPowerArmorAllowed != None
        bInCooldown = True
        EN05_Basic_NoPowerArmorAllowed.Show()
        StartTimer(fPAMessageCooldownLength, iPAMessageCooldownID)
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == iPAMessageCooldownID
        bInCooldown = False
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(iPAMessageCooldownID)
    bInCooldown = False
    bKickOffStartUpScene = False
    If EN05_Basic_0050_ProcessCourseCompletion != None
        EN05_Basic_0050_ProcessCourseCompletion.Stop()
    EndIf
EndEvent
