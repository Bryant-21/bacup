MTNM04QuestScript Function EventScript()
    Quest owner = Self as Quest
    Return owner as MTNM04QuestScript
EndFunction

Function ResetEventObjectives()
    Int[] objectives = New Int[13]
    objectives[0] = 100
    objectives[1] = 150
    objectives[2] = 200
    objectives[3] = 210
    objectives[4] = 211
    objectives[5] = 212
    objectives[6] = 213
    objectives[7] = 214
    objectives[8] = 215
    objectives[9] = 251
    objectives[10] = 265
    objectives[11] = 275
    objectives[12] = 350
    Int index = 0
    While index < objectives.Length
        SetObjectiveDisplayed(objectives[index], False)
        SetObjectiveCompleted(objectives[index], False)
        SetObjectiveFailed(objectives[index], False)
        index += 1
    EndWhile
EndFunction

Function FailOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective)
        SetObjectiveFailed(aiObjective, True)
    EndIf
EndFunction

Function HideOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective)
        SetObjectiveDisplayed(aiObjective, False)
    EndIf
EndFunction

Function DestroyRobot(Int aiIndex)
    MTNM04QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.DestroyRobot(aiIndex)
    EndIf
EndFunction

Function StartWavePhase(Int aiStage)
    MTNM04QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.StartWavePhase(aiStage)
    EndIf
EndFunction

Function CheckTablesDone()
    If IsStageDone(266) && IsStageDone(276) && !IsStageDone(325)
        SetObjectiveCompleted(251, True)
        SetStage(325)
    EndIf
EndFunction

Function FinishEvent(Bool abSuccess)
    If abSuccess
        SetObjectiveCompleted(150, True)
        SetObjectiveCompleted(200, True)
        HideOpenObjective(251)
        HideOpenObjective(265)
        HideOpenObjective(275)
    Else
        FailOpenObjective(100)
        FailOpenObjective(150)
        FailOpenObjective(200)
        FailOpenObjective(210)
        FailOpenObjective(251)
        FailOpenObjective(265)
        FailOpenObjective(275)
    EndIf
    MTNM04QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.EndActivity()
        eventScript.ScheduleShutdown(15.0)
    Else
        Stop()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    ResetEventObjectives()
    MTNM04QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.ResetActivity()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_01()
    SetObjectiveDisplayed(100, True, True)
EndFunction

; Set when the player activates Billingsley.
Function Fragment_Stage_0200_Item_00()
    If IsStageDone(9990)
        Return
    EndIf
    SetObjectiveCompleted(100, True)
    MTNM04QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.PlayIntroScene()
    ElseIf !IsStageDone(230)
        SetStage(230)
    EndIf
EndFunction

Function Fragment_Stage_0230_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(150, True, True)
    SetObjectiveDisplayed(200, True, True)
    SetObjectiveDisplayed(210, True, True)
    MTNM04QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.StartRobotPhase()
    EndIf
EndFunction

; B21:ObjectiveTimers sets 231-235 when a downed waiter is not repaired in time.
Function Fragment_Stage_0231_Item_00()
    DestroyRobot(0)
EndFunction

Function Fragment_Stage_0232_Item_00()
    DestroyRobot(1)
EndFunction

Function Fragment_Stage_0233_Item_00()
    DestroyRobot(2)
EndFunction

Function Fragment_Stage_0234_Item_00()
    DestroyRobot(3)
EndFunction

Function Fragment_Stage_0235_Item_00()
    DestroyRobot(4)
EndFunction

Function Fragment_Stage_0250_Item_00()
    SetObjectiveCompleted(210, True)
    SetObjectiveDisplayed(251, True, True)
    SetObjectiveDisplayed(265, True, True)
    SetObjectiveDisplayed(275, True, True)
    MTNM04QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.StartTablePhase()
    EndIf
    If !IsStageDone(255)
        SetStage(255)
    EndIf
EndFunction

Function Fragment_Stage_0255_Item_00()
    MTNM04QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.StartUndesirableTracking()
    EndIf
EndFunction

Function Fragment_Stage_0266_Item_00()
    SetObjectiveCompleted(265, True)
    CheckTablesDone()
EndFunction

Function Fragment_Stage_0276_Item_00()
    SetObjectiveCompleted(275, True)
    CheckTablesDone()
EndFunction

Function Fragment_Stage_0300_Item_00()
    StartWavePhase(300)
EndFunction

Function Fragment_Stage_0305_Item_00()
    StartWavePhase(305)
EndFunction

Function Fragment_Stage_0310_Item_00()
    StartWavePhase(310)
EndFunction

Function Fragment_Stage_0315_Item_00()
    StartWavePhase(315)
EndFunction

Function Fragment_Stage_0320_Item_00()
    StartWavePhase(320)
EndFunction

Function Fragment_Stage_0325_Item_00()
    If MTNM04_PA_EarlyCompletionScene != None && !MTNM04_PA_EarlyCompletionScene.IsPlaying()
        MTNM04_PA_EarlyCompletionScene.Start()
    EndIf
    MTNM04QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.StartEarlyCompletion()
    EndIf
EndFunction

; The boss wave sets this stage once the late-arriving boss is in the world.
Function Fragment_Stage_0326_Item_00()
    MTNM04QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.RefreshUndesirables()
    EndIf
EndFunction

; B21:QuestTimer sets this stage when the gala begins.
Function Fragment_Stage_0400_Item_00()
    If IsStageDone(9990) || IsStageDone(525)
        Return
    EndIf
    MTNM04QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.EndActivity()
        eventScript.PlayEndScene()
    ElseIf !IsStageDone(550)
        SetStage(550)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_01()
    Quest owner = Self as Quest
    DefaultQuestEncounterWaveScript waves = owner as DefaultQuestEncounterWaveScript
    If waves != None
        waves.StopAllEncounterWaves(False)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    FinishEvent(True)
EndFunction

Function Fragment_Stage_0525_Item_00()
    Quest owner = Self as Quest
    DefaultQuestEncounterWaveScript waves = owner as DefaultQuestEncounterWaveScript
    If waves != None
        waves.StopAllEncounterWaves(True)
    EndIf
    MTNM04QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.EndActivity()
    EndIf
    Stop()
EndFunction

Function Fragment_Stage_0550_Item_00()
    If IsStageDone(500)
        Return
    EndIf
    FinishEvent(False)
EndFunction

; B21:ObjectiveTimers sets this stage when nobody spoke to Billingsley during the prep window.
Function Fragment_Stage_9990_Item_00()
    If IsStageDone(200)
        Return
    EndIf
    FinishEvent(False)
EndFunction
