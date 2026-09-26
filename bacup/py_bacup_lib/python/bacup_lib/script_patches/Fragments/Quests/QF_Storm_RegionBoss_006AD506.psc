Quests:Storm:RegionBoss:RegionBossQuestScript Function GetBossEvent()
    Quest owner = Self as Quest
    Return owner as Quests:Storm:RegionBoss:RegionBossQuestScript
EndFunction

Function ResetObjective(Int aiObjective)
    SetObjectiveDisplayed(aiObjective, False)
    SetObjectiveCompleted(aiObjective, False)
    SetObjectiveFailed(aiObjective, False)
EndFunction

Function ResetEventObjectives()
    ResetObjective(5)
    ResetObjective(10)
    ResetObjective(20)
    ResetObjective(21)
    ResetObjective(22)
    ResetObjective(23)
EndFunction

Function FailOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective)
        SetObjectiveFailed(aiObjective, True)
    EndIf
EndFunction

Function DisplayBossObjective(Int aiObjective, ReferenceAlias akBossAlias)
    If akBossAlias != None && akBossAlias.GetReference() != None
        SetObjectiveDisplayed(aiObjective, True)
    EndIf
EndFunction

ObjectReference Function GetEntranceDoor()
    If Alias_EntranceDoor == None
        Return None
    EndIf
    Return Alias_EntranceDoor.GetReference()
EndFunction

Function StartEventScene(Scene akScene)
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    ResetEventObjectives()
    ObjectReference entranceDoor = GetEntranceDoor()
    Quests:Storm:RegionBoss:RegionBossQuestScript bossEvent = GetBossEvent()
    If bossEvent != None
        bossEvent.CaptureEntranceDoor(entranceDoor)
    EndIf
    If entranceDoor != None && !entranceDoor.IsLocked()
        ; A repeat run starts with the arena sealed until the timer or the terminal bypass opens it.
        entranceDoor.SetLockLevel(254)
        entranceDoor.Lock(True)
    EndIf
    SetObjectiveDisplayed(5, True, True)
EndFunction

Function Fragment_Stage_0105_Item_00()
    If IsStageDone(110)
        Return
    EndIf
    StartEventScene(Scene_ARadio_DoorByPass)
    SetStage(110)
EndFunction

Function Fragment_Stage_0110_Item_00()
    SetObjectiveCompleted(5, True)
    ObjectReference entranceDoor = GetEntranceDoor()
    If entranceDoor != None && entranceDoor.IsLocked()
        entranceDoor.Unlock()
    EndIf
    If Scene_ARadio_DoorByPass == None || !Scene_ARadio_DoorByPass.IsPlaying()
        StartEventScene(Storm_RegionBoss_DoorOpens)
    EndIf
    SetObjectiveDisplayed(10, True, True)
    Quests:Storm:RegionBoss:RegionBossQuestScript bossEvent = GetBossEvent()
    If bossEvent != None
        bossEvent.StartTimerCallouts()
    EndIf
EndFunction

Function Fragment_Stage_0115_Item_00()
    SetObjectiveCompleted(10, True)
    Quests:Storm:RegionBoss:RegionBossQuestScript bossEvent = GetBossEvent()
    If bossEvent == None || !bossEvent.ReleaseBosses()
        SetStage(9990)
        Return
    EndIf
    SetObjectiveDisplayed(20, True, True)
    DisplayBossObjective(21, Alias_Boss_01)
    DisplayBossObjective(22, Alias_Boss_02)
    DisplayBossObjective(23, Alias_Boss_03)
    StartEventScene(Storm_RegionBoss_Wave1_Start)
    If bossEvent != None
        bossEvent.StartMobWave("MobSpawn_Eyebots")
        bossEvent.StartMobWave("MobSpawn_RoboBrains")
        bossEvent.StartLaserGridCycle()
    EndIf
    If !IsStageDone(120)
        SetStage(120)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    Quests:Storm:RegionBoss:RegionBossQuestScript bossEvent = GetBossEvent()
    If bossEvent == None || bossEvent.EventResolved()
        Return
    EndIf
    bossEvent.EmpowerRemainingBosses(1.0)
    If bossEvent.CountLivingBosses() > 0
        StartEventScene(Storm_RegionBoss_Wave_Next)
        If Storm_RegionBoss_BossBuff_Message != None
            Storm_RegionBoss_BossBuff_Message.Show()
        EndIf
        bossEvent.StartMobWave("MobSpawn_Lost")
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    Quests:Storm:RegionBoss:RegionBossQuestScript bossEvent = GetBossEvent()
    If bossEvent == None || bossEvent.EventResolved()
        Return
    EndIf
    bossEvent.EmpowerRemainingBosses(2.0)
    If bossEvent.CountLivingBosses() > 0
        StartEventScene(Storm_RegionBoss_Wave_Next)
        If Storm_RegionBoss_BossBuff_Singular_Message != None
            Storm_RegionBoss_BossBuff_Singular_Message.Show()
        EndIf
        bossEvent.StartMobWave("MobSpawn_LostHeavy")
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    If IsStageDone(9990)
        Return
    EndIf
    SetObjectiveCompleted(20, True)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_5000_Item_00()
    Quests:Storm:RegionBoss:RegionBossQuestScript bossEvent = GetBossEvent()
    If bossEvent != None
        bossEvent.CleanupArena()
    EndIf
    If !IsStageDone(500) && !IsStageDone(9000) && !IsStageDone(9990)
        SetStage(9990)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(20, True)
    Quests:Storm:RegionBoss:RegionBossQuestScript bossEvent = GetBossEvent()
    If bossEvent != None
        bossEvent.StopQuestTimer()
    EndIf
    StartEventScene(Storm_RegionBoss_Victory)
    If !IsStageDone(5000)
        SetStage(5000)
    EndIf
    If bossEvent != None
        bossEvent.ScheduleEventStop(9999, 20.0)
    EndIf
EndFunction

Function Fragment_Stage_9990_Item_00()
    FailOpenObjective(5)
    FailOpenObjective(10)
    FailOpenObjective(20)
    FailOpenObjective(21)
    FailOpenObjective(22)
    FailOpenObjective(23)
    Quests:Storm:RegionBoss:RegionBossQuestScript bossEvent = GetBossEvent()
    If bossEvent != None
        bossEvent.StopQuestTimer()
        bossEvent.SelfDestructBosses()
    EndIf
    StartEventScene(Storm_RegionBoss_Failure)
    If !IsStageDone(5000)
        SetStage(5000)
    EndIf
    If bossEvent != None
        bossEvent.ScheduleEventStop(9999, 20.0)
    EndIf
EndFunction

Function Fragment_Stage_9999_Item_00()
    Stop()
EndFunction
