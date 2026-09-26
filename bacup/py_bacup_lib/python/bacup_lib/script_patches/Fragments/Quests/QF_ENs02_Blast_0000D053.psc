; Debug stage: killed every spawned enemy on the FO76 server. No-op here.
Function Fragment_Stage_0002_Item_00()
EndFunction

Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10)
    ENs02_BlastQuestScript controller = (Self as Quest) as ENs02_BlastQuestScript
    If controller != None
        controller.ENs02_BeginStartup()
    EndIf
EndFunction

Function Fragment_Stage_0015_Item_00()
    ENs02_BlastQuestScript controller = (Self as Quest) as ENs02_BlastQuestScript
    If controller != None
        controller.ENs02_ReleaseVertibot()
    EndIf
EndFunction

Function Fragment_Stage_0017_Item_00()
    ENs02_BlastQuestScript controller = (Self as Quest) as ENs02_BlastQuestScript
    If controller != None
        controller.ENs02_ReleaseVertibot()
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(50)
    ENs02_BlastQuestScript controller = (Self as Quest) as ENs02_BlastQuestScript
    If controller != None
        controller.ENs02_BeginDefense()
    EndIf
EndFunction

Function Fragment_Stage_0058_Item_00()
    ENs02_WaveSpawned(0)
EndFunction

Function Fragment_Stage_0063_Item_00()
    ENs02_WaveSpawned(1)
EndFunction

Function Fragment_Stage_0068_Item_00()
    ENs02_WaveSpawned(2)
EndFunction

Function Fragment_Stage_0073_Item_00()
    ENs02_WaveSpawned(3)
EndFunction

Function Fragment_Stage_0078_Item_00()
    ENs02_WaveSpawned(4)
EndFunction

Function Fragment_Stage_0083_Item_00()
    ENs02_WaveSpawned(5)
EndFunction

Function Fragment_Stage_0100_Item_00()
    If IsObjectiveDisplayed(60)
        SetObjectiveCompleted(60)
    EndIf
    SetObjectiveCompleted(50)
    ENs02_BlastQuestScript controller = (Self as Quest) as ENs02_BlastQuestScript
    If controller != None
        controller.ENs02_WarnFlee()
    EndIf
EndFunction

Function Fragment_Stage_0110_Item_00()
    SetObjectiveDisplayed(100)
    ENs02_BlastQuestScript controller = (Self as Quest) as ENs02_BlastQuestScript
    If controller != None
        controller.ENs02_StartFleeTimer()
    EndIf
EndFunction

Function Fragment_Stage_0120_Item_00()
    SetObjectiveDisplayed(120)
    ENs02_BlastQuestScript controller = (Self as Quest) as ENs02_BlastQuestScript
    If controller != None && controller.ENs02_LivingEnemies() == 0
        SetObjectiveCompleted(120)
        SetStage(150)
    EndIf
EndFunction

Function Fragment_Stage_0123_Item_00()
    SetObjectiveCompleted(100)
    ENs02_BlastQuestScript controller = (Self as Quest) as ENs02_BlastQuestScript
    If controller != None
        controller.ENs02_LaunchStrike()
    EndIf
EndFunction

Function Fragment_Stage_0145_Item_00()
    If IsStageDone(120)
        SetStage(150)
        Return
    EndIf
    ENs02_BlastQuestScript controller = (Self as Quest) as ENs02_BlastQuestScript
    If controller != None
        controller.ENs02_CheckOrientationComplete()
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    If IsObjectiveDisplayed(120)
        SetObjectiveCompleted(120)
    EndIf
    ENs02_Say(ENs02_AreaSecure)
    EnclaveEventQuestScript eventQuest = (Self as Quest) as EnclaveEventQuestScript
    If eventQuest != None
        eventQuest.ENEvent_RecordCompletion()
    EndIf
    ENs02_BlastQuestScript controller = (Self as Quest) as ENs02_BlastQuestScript
    If controller != None
        controller.ENs02_BeginWrapUp()
    EndIf
EndFunction

Function Fragment_Stage_0160_Item_00()
    ENs02_Say(ENs02_FailureLine)
    Int[] objectives = new Int[5]
    objectives[0] = 10
    objectives[1] = 50
    objectives[2] = 60
    objectives[3] = 100
    objectives[4] = 120
    Int index = 0
    While index < objectives.Length
        If IsObjectiveDisplayed(objectives[index]) && !IsObjectiveCompleted(objectives[index])
            SetObjectiveFailed(objectives[index])
        EndIf
        index += 1
    EndWhile
    If !IsStageDone(161)
        SetStage(161)
    EndIf
    ENs02_BlastQuestScript controller = (Self as Quest) as ENs02_BlastQuestScript
    If controller != None
        controller.ENs02_BeginWrapUp()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    Stop()
EndFunction

Function Fragment_Stage_0999_Item_00()
    ENs02_BlastQuestScript controller = (Self as Quest) as ENs02_BlastQuestScript
    If controller != None
        controller.ENs02_Shutdown()
    EndIf
EndFunction

Function ENs02_WaveSpawned(Int aiWaveIndex)
    ENs02_BlastQuestScript controller = (Self as Quest) as ENs02_BlastQuestScript
    If controller != None
        controller.ENs02_WaveSpawned(aiWaveIndex)
    EndIf
EndFunction

Function ENs02_Say(Topic akTopic)
    EnclaveEventQuestScript eventQuest = (Self as Quest) as EnclaveEventQuestScript
    If eventQuest != None
        eventQuest.ENEvent_SayToPlayer(akTopic)
    EndIf
EndFunction
