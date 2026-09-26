MTR05_BreachQuestScript Function EventScript()
    Quest owner = Self as Quest
    Return owner as MTR05_BreachQuestScript
EndFunction

MTR05_EnemyDeathCounter Function DeathCounter()
    Return Alias_ActiveEnemies as MTR05_EnemyDeathCounter
EndFunction

Function CompleteObjectiveIfOpen(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveCompleted(aiObjective, True)
    EndIf
EndFunction

Function FailObjectiveIfOpen(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveFailed(aiObjective, True)
    EndIf
EndFunction

Int[] Function BreachObjectives()
    Int[] objectives = New Int[5]
    objectives[0] = 25
    objectives[1] = 45
    objectives[2] = 50
    objectives[3] = 60
    objectives[4] = 70
    Return objectives
EndFunction

Function ResetBreachObjectives()
    Int[] objectives = BreachObjectives()
    Int index = 0
    While index < objectives.Length
        SetObjectiveDisplayed(objectives[index], False)
        SetObjectiveCompleted(objectives[index], False)
        SetObjectiveFailed(objectives[index], False)
        index += 1
    EndWhile
EndFunction

Function CloseOpenObjectives(Bool abFailed)
    Int[] objectives = BreachObjectives()
    Int index = 0
    While index < objectives.Length
        If abFailed
            FailObjectiveIfOpen(objectives[index])
        Else
            CompleteObjectiveIfOpen(objectives[index])
        EndIf
        index += 1
    EndWhile
EndFunction

Function SetPlayerTriggerEnabled(Bool abEnabled)
    If Alias_ActivePlayerTrigger == None
        Return
    EndIf
    ObjectReference triggerRef = Alias_ActivePlayerTrigger.GetReference()
    If triggerRef == None
        Return
    EndIf
    If abEnabled
        triggerRef.Enable(False)
    Else
        triggerRef.Disable(False)
    EndIf
EndFunction

Function AdvanceWhenAllWavesSpawned()
    If GetStageDone(20) && GetStageDone(21) && !GetStageDone(25)
        SetStage(25)
    EndIf
EndFunction

Function AdvanceWhenAllWavesThinned()
    If GetStageDone(30) && GetStageDone(31) && !GetStageDone(35)
        SetStage(35)
    EndIf
EndFunction

Function EvaluateEnemiesRemaining()
    MTR05_EnemyDeathCounter deathCounter = DeathCounter()
    If deathCounter != None
        deathCounter.EvaluateEnemiesRemaining()
    EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
    ResetBreachObjectives()
    SetPlayerTriggerEnabled(True)
    MTR05_BreachQuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.StartBreachEvent()
    EndIf
    SetObjectiveDisplayed(25, True, True)
    ; FO76's server set stage 15 once the site had streamed in; the objectives above are
    ; the work that stage was there to kick off.
    If !GetStageDone(15)
        SetStage(15)
    EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
    AdvanceWhenAllWavesSpawned()
EndFunction

Function Fragment_Stage_0021_Item_00()
    AdvanceWhenAllWavesSpawned()
EndFunction

Function Fragment_Stage_0022_Item_00()
    AdvanceWhenAllWavesSpawned()
EndFunction

Function Fragment_Stage_0023_Item_00()
    AdvanceWhenAllWavesSpawned()
EndFunction

Function Fragment_Stage_0025_Item_00()
    SetObjectiveDisplayed(25, True, True)
    ; Stage 26 is the counter's prerequisite, so the remaining-enemy tracking starts here.
    If !GetStageDone(26)
        SetStage(26)
    EndIf
EndFunction

Function Fragment_Stage_0026_Item_00()
    EvaluateEnemiesRemaining()
EndFunction

Function Fragment_Stage_0030_Item_00()
    AdvanceWhenAllWavesThinned()
EndFunction

Function Fragment_Stage_0031_Item_00()
    AdvanceWhenAllWavesThinned()
EndFunction

Function Fragment_Stage_0032_Item_00()
    AdvanceWhenAllWavesThinned()
EndFunction

Function Fragment_Stage_0033_Item_00()
    AdvanceWhenAllWavesThinned()
EndFunction

Function Fragment_Stage_0040_Item_00()
    EvaluateEnemiesRemaining()
EndFunction

Function Fragment_Stage_0041_Item_00()
    EvaluateEnemiesRemaining()
EndFunction

Function Fragment_Stage_0042_Item_00()
    EvaluateEnemiesRemaining()
EndFunction

Function Fragment_Stage_0043_Item_00()
    EvaluateEnemiesRemaining()
EndFunction

Function Fragment_Stage_0045_Item_00()
    CompleteObjectiveIfOpen(25)
    MTR05_BreachQuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.StopBreachWaves(False)
        eventScript.PlaySceneIfIdle(eventScript.MTR05_Mother_Breach_0045_BreachWarning)
    EndIf
    ; Objective 45's own timer (15 s) is what sets stage 47, so the breach follows the
    ; retreat warning without a hand-rolled timer.
    SetObjectiveDisplayed(45, True, True)
EndFunction

Function Fragment_Stage_0047_Item_00()
    CompleteObjectiveIfOpen(45)
    MTR05_BreachQuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.BeginBreach()
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    CompleteObjectiveIfOpen(25)
    CompleteObjectiveIfOpen(45)
    MTR05_BreachQuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.BeginContainerCollection()
    EndIf
    SetObjectiveDisplayed(50, True, True)
EndFunction

Function Fragment_Stage_0070_Item_00()
    MTR05_BreachQuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.StopContainerCollection()
        eventScript.PlaySceneIfIdle(eventScript.MTR05_Mother_Breach_0070_InitiatingSubmerge)
    EndIf
    CompleteObjectiveIfOpen(50)
    SetObjectiveDisplayed(60, False)
    ; Objective 70's timer (30 s) sets stage 100 when the last collection window closes.
    SetObjectiveDisplayed(70, True, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    CompleteObjectiveIfOpen(70)
    If !GetStageDone(101)
        SetStage(101)
    EndIf
    MTR05_BreachQuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.StopContainerCollection()
        eventScript.StopBreachWaves(False)
        eventScript.PlaySceneIfIdle(eventScript.MTR05_Mother_Breach_0100_Submerge)
        eventScript.ScheduleCompletion()
    ElseIf !GetStageDone(200)
        SetStage(200)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    CloseOpenObjectives(False)
    SetPlayerTriggerEnabled(False)
    MTR05_BreachQuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.StopContainerCollection()
        eventScript.StopBreachWaves(False)
        ; Let the completion rewards and the quest notification land before shutdown.
        eventScript.ScheduleShutdown(5.0)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    CloseOpenObjectives(True)
    If !GetStageDone(101)
        SetStage(101)
    EndIf
    MTR05_BreachQuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.StopContainerCollection()
        eventScript.StopBreachWaves(False)
        eventScript.PlaySceneIfIdle(MTR05_Mother_Breach_0300_Failure)
        eventScript.ScheduleFailureShutdown(10.0)
    ElseIf !GetStageDone(305)
        SetStage(305)
    EndIf
EndFunction

Function Fragment_Stage_0305_Item_00()
    MTR05_BreachQuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.CleanupBreach(True)
    EndIf
    SetPlayerTriggerEnabled(False)
    If !IsStopping() && !IsStopped()
        Stop()
    EndIf
EndFunction

Function Fragment_Stage_0999_Item_00()
    MTR05_BreachQuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.CleanupBreach(True)
    EndIf
    SetPlayerTriggerEnabled(False)
    If Alias_EnableMarker != None && Alias_EnableMarker.GetReference() != None
        Alias_EnableMarker.GetReference().Disable(False)
    EndIf
    If Alias_MapMarker != None && Alias_MapMarker.GetReference() != None
        Alias_MapMarker.GetReference().Disable(False)
    EndIf
EndFunction
