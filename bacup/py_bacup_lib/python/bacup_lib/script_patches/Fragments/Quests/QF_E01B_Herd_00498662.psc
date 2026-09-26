Quests:E01B_Herd:QuestScript Function GetHerdEventScript()
    Quest owner = Self as Quest
    Return owner as Quests:E01B_Herd:QuestScript
EndFunction

Function ResetHerdObjectives()
    Int[] objectives = New Int[7]
    objectives[0] = 10
    objectives[1] = 15
    objectives[2] = 20
    objectives[3] = 21
    objectives[4] = 22
    objectives[5] = 23
    objectives[6] = 25
    Int index = 0
    While index < objectives.Length
        SetObjectiveDisplayed(objectives[index], False)
        SetObjectiveCompleted(objectives[index], False)
        SetObjectiveFailed(objectives[index], False)
        index += 1
    EndWhile
EndFunction

Function CompleteOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveCompleted(aiObjective, True)
    EndIf
EndFunction

Function FailOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveFailed(aiObjective, True)
    EndIf
EndFunction

Function DisplayBrahminObjective(Int aiObjective, Actor akBrahmin)
    If akBrahmin != None && !akBrahmin.IsDead() && !IsObjectiveFailed(aiObjective)
        SetObjectiveDisplayed(aiObjective, True)
    EndIf
EndFunction

Function FailHerdEvent()
    FailOpenObjective(10)
    FailOpenObjective(15)
    FailOpenObjective(20)
    FailOpenObjective(21)
    FailOpenObjective(22)
    FailOpenObjective(23)
    FailOpenObjective(25)
    Quests:E01B_Herd:QuestScript eventScript = GetHerdEventScript()
    If eventScript != None
        eventScript.CleanupEvent()
    EndIf
    Stop()
EndFunction

Function Fragment_Stage_0100_Item_00()
    ResetHerdObjectives()
    SetObjectiveDisplayed(10, True, True)
    Quests:E01B_Herd:QuestScript eventScript = GetHerdEventScript()
    If eventScript != None
        eventScript.SetupEvent()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    CompleteOpenObjective(10)
    If !IsStageDone(205)
        SetObjectiveDisplayed(15, True, True)
    EndIf
    Quests:E01B_Herd:QuestScript eventScript = GetHerdEventScript()
    If eventScript != None
        eventScript.OnPlayerAcquiredCrook()
    EndIf
EndFunction

Function Fragment_Stage_0205_Item_00()
    CompleteOpenObjective(10)
    CompleteOpenObjective(15)
    SetObjectiveDisplayed(20, True, True)
    Quests:E01B_Herd:QuestScript eventScript = GetHerdEventScript()
    If eventScript == None
        Return
    EndIf
    DisplayBrahminObjective(21, eventScript.GetBrahmin(0))
    DisplayBrahminObjective(22, eventScript.GetBrahmin(1))
    DisplayBrahminObjective(23, eventScript.GetBrahmin(2))
    eventScript.AdvanceHerdGoal(0)
EndFunction

Function Fragment_Stage_0210_Item_00()
    GetHerdEventScript().CheckHerdCheckpoint(210, 220, 230, 300)
EndFunction

Function Fragment_Stage_0220_Item_00()
    GetHerdEventScript().CheckHerdCheckpoint(210, 220, 230, 300)
EndFunction

Function Fragment_Stage_0230_Item_00()
    GetHerdEventScript().CheckHerdCheckpoint(210, 220, 230, 300)
EndFunction

Function Fragment_Stage_0300_Item_00()
    Quests:E01B_Herd:QuestScript eventScript = GetHerdEventScript()
    If eventScript == None
        Return
    EndIf
    eventScript.StartHerdWave(1)
    eventScript.AdvanceHerdGoal(1)
EndFunction

Function Fragment_Stage_0301_Item_00()
    GetHerdEventScript().CheckHerdCheckpoint(301, 302, 303, 305)
EndFunction

Function Fragment_Stage_0302_Item_00()
    GetHerdEventScript().CheckHerdCheckpoint(301, 302, 303, 305)
EndFunction

Function Fragment_Stage_0303_Item_00()
    GetHerdEventScript().CheckHerdCheckpoint(301, 302, 303, 305)
EndFunction

; The Sheepsquatch howl scares Brahmin 3 off the road until the player herds it back (307/308).
Function Fragment_Stage_0305_Item_00()
    Quests:E01B_Herd:QuestScript eventScript = GetHerdEventScript()
    If eventScript == None
        Return
    EndIf
    eventScript.PlayScareEffects()
    eventScript.AdvanceHerdGoal(2)

    Actor strayBrahmin = None
    If Alias_Actor_Brahmin_03 != None
        strayBrahmin = Alias_Actor_Brahmin_03.GetActorReference()
    EndIf
    ObjectReference offroadMarker = None
    If Alias_Marker_Path_Offroad != None
        offroadMarker = Alias_Marker_Path_Offroad.GetReference()
    EndIf
    If strayBrahmin == None || strayBrahmin.IsDead() || offroadMarker == None
        Return
    EndIf
    eventScript.MoveTravelMarker(strayBrahmin, offroadMarker)
    Quests:E01B_Herd:HerdScript strayHerd = Alias_Actor_Brahmin_03 as Quests:E01B_Herd:HerdScript
    If strayHerd != None
        strayHerd.StartPanic()
    EndIf
EndFunction

Function Fragment_Stage_0307_Item_00()
    Quests:E01B_Herd:HerdScript strayHerd = Alias_Actor_Brahmin_03 as Quests:E01B_Herd:HerdScript
    If strayHerd != None
        strayHerd.ClearScaredKeyword()
    EndIf
EndFunction

Function Fragment_Stage_0308_Item_00()
    Quests:E01B_Herd:QuestScript eventScript = GetHerdEventScript()
    If eventScript != None && Alias_Actor_Brahmin_03 != None
        eventScript.ReturnStrayBrahmin(Alias_Actor_Brahmin_03.GetActorReference())
    EndIf
EndFunction

Function Fragment_Stage_0310_Item_00()
    GetHerdEventScript().CheckHerdCheckpoint(310, 320, 330, 400)
EndFunction

Function Fragment_Stage_0320_Item_00()
    GetHerdEventScript().CheckHerdCheckpoint(310, 320, 330, 400)
EndFunction

Function Fragment_Stage_0330_Item_00()
    GetHerdEventScript().CheckHerdCheckpoint(310, 320, 330, 400)
EndFunction

Function Fragment_Stage_0400_Item_00()
    Quests:E01B_Herd:QuestScript eventScript = GetHerdEventScript()
    If eventScript == None
        Return
    EndIf
    eventScript.StartHerdWave(2)
    eventScript.AdvanceHerdGoal(3)
EndFunction

Function Fragment_Stage_0410_Item_00()
    GetHerdEventScript().CheckHerdCheckpoint(410, 420, 430, 500)
EndFunction

Function Fragment_Stage_0420_Item_00()
    GetHerdEventScript().CheckHerdCheckpoint(410, 420, 430, 500)
EndFunction

Function Fragment_Stage_0430_Item_00()
    GetHerdEventScript().CheckHerdCheckpoint(410, 420, 430, 500)
EndFunction

Function Fragment_Stage_0500_Item_00()
    Quests:E01B_Herd:QuestScript eventScript = GetHerdEventScript()
    If eventScript == None
        Return
    EndIf
    eventScript.StartHerdWave(3)
    eventScript.AdvanceHerdGoal(4)
EndFunction

Function Fragment_Stage_0510_Item_00()
    GetHerdEventScript().CheckHerdCheckpoint(510, 520, 530, 600)
EndFunction

Function Fragment_Stage_0520_Item_00()
    GetHerdEventScript().CheckHerdCheckpoint(510, 520, 530, 600)
EndFunction

Function Fragment_Stage_0530_Item_00()
    GetHerdEventScript().CheckHerdCheckpoint(510, 520, 530, 600)
EndFunction

Function Fragment_Stage_0600_Item_00()
    CompleteOpenObjective(20)
    SetObjectiveDisplayed(25, True, True)
    If E01B_Herd_Message_UnderAttackSheepsquatch != None
        E01B_Herd_Message_UnderAttackSheepsquatch.Show()
    EndIf
    Quests:E01B_Herd:QuestScript eventScript = GetHerdEventScript()
    If eventScript == None
        Return
    EndIf
    eventScript.AdvanceHerdGoal(5)
    eventScript.StartBossFight()
EndFunction

Function Fragment_Stage_0910_Item_00()
    GetHerdEventScript().HandleBrahminDeath(21)
EndFunction

Function Fragment_Stage_0920_Item_00()
    GetHerdEventScript().HandleBrahminDeath(22)
EndFunction

Function Fragment_Stage_0930_Item_00()
    GetHerdEventScript().HandleBrahminDeath(23)
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteOpenObjective(20)
    CompleteOpenObjective(21)
    CompleteOpenObjective(22)
    CompleteOpenObjective(23)
    CompleteOpenObjective(25)
    Quests:E01B_Herd:QuestScript eventScript = GetHerdEventScript()
    If eventScript == None
        Return
    EndIf
    eventScript.BrahminStillAlive = eventScript.CountLivingBrahmin()
    eventScript.StopHerdWaves()
    ; Leave time for stage 9000 rewards and the Bigfoot party crasher before shutdown.
    eventScript.ScheduleShutdown(30.0)
EndFunction

Function Fragment_Stage_9990_Item_00()
    FailHerdEvent()
EndFunction

Function Fragment_Stage_9991_Item_00()
    FailHerdEvent()
EndFunction

Function Fragment_Stage_9992_Item_00()
    FailHerdEvent()
EndFunction
