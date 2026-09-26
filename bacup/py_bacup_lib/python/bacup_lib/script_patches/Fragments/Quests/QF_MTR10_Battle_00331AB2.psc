MTR10_BattleQuestScript Function EventScript()
    Quest owner = Self as Quest
    Return owner as MTR10_BattleQuestScript
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

Int[] Function BattleObjectives()
    Int[] objectives = New Int[8]
    objectives[0] = 10
    objectives[1] = 20
    objectives[2] = 160
    objectives[3] = 161
    objectives[4] = 180
    objectives[5] = 181
    objectives[6] = 182
    objectives[7] = 190
    Return objectives
EndFunction

Function ResetBattleObjectives()
    Int[] objectives = BattleObjectives()
    Int index = 0
    While index < objectives.Length
        SetObjectiveDisplayed(objectives[index], False)
        SetObjectiveCompleted(objectives[index], False)
        SetObjectiveFailed(objectives[index], False)
        index += 1
    EndWhile
EndFunction

Function CloseOpenObjectives(Bool abFailed)
    Int[] objectives = BattleObjectives()
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

Function Fragment_Stage_0010_Item_00()
    MTR10_BattleQuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.ResetKeypads()
    EndIf
    ResetBattleObjectives()
    SetObjectiveDisplayed(10, True, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    CompleteObjectiveIfOpen(10)
    SetObjectiveDisplayed(20, True, True)

    ObjectReference robotRef = None
    If Alias_Robot01 != None
        robotRef = Alias_Robot01.GetReference()
    EndIf
    ; The bot has to survive the trip home, which is what its damage-resist ability is for.
    If robotRef != None && MTR10RobotAbility01 != None
        MTR10RobotAbility01.Cast(robotRef, robotRef)
    EndIf
    If robotRef != None && MTR10_Battle_Robot_RetreatTopic != None
        robotRef.Say(MTR10_Battle_Robot_RetreatTopic)
    EndIf

    MTR10_BattleQuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.BeginRetreatWatch()
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    CompleteObjectiveIfOpen(10)
    CompleteObjectiveIfOpen(20)
    ; Objective 160's own target conditions point it at the bunker until stage 160 opens it.
    SetObjectiveDisplayed(160, True, True)
EndFunction

Function Fragment_Stage_0160_Item_00()
    CompleteObjectiveIfOpen(10)
    CompleteObjectiveIfOpen(20)
    MTR10_BattleQuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.OpenBunker(Alias_BunkerDoor)
    EndIf
    SetObjectiveDisplayed(160, True, True)
    SetObjectiveDisplayed(161, True, True)
EndFunction

Function Fragment_Stage_0170_Item_00()
    CompleteObjectiveIfOpen(160)
    If GetStageDone(171) && !GetStageDone(180)
        SetStage(180)
    EndIf
EndFunction

Function Fragment_Stage_0171_Item_00()
    CompleteObjectiveIfOpen(161)
    If GetStageDone(170) && !GetStageDone(180)
        SetStage(180)
    EndIf
EndFunction

Function Fragment_Stage_0180_Item_00()
    CompleteObjectiveIfOpen(160)
    CompleteObjectiveIfOpen(161)
    MTR10_BattleQuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.ResetKeypads()
    EndIf
    SetObjectiveDisplayed(180, True, True)
    SetObjectiveDisplayed(181, True, True)
EndFunction

Function Fragment_Stage_0190_Item_00()
    CompleteObjectiveIfOpen(180)
    CompleteObjectiveIfOpen(181)
    CompleteObjectiveIfOpen(182)
    SetObjectiveDisplayed(190, True, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    CloseOpenObjectives(False)
    MTR10_BattleQuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.FinishBattle()
    ElseIf !GetStageDone(500)
        SetStage(500)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    ; The 15 minute quest timer expired. FO76 flagged both 300 and 400 TimerEnd, so route
    ; a finished run straight to shutdown instead of letting 400 fail a completed quest.
    If GetStageDone(200) || IsCompleted()
        If !GetStageDone(500)
            SetStage(500)
        EndIf
    ElseIf !GetStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    CloseOpenObjectives(True)
    If !GetStageDone(500)
        SetStage(500)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    MTR10_BattleQuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.CleanupBattle()
    EndIf
    If !IsStopping() && !IsStopped()
        Stop()
    EndIf
EndFunction
