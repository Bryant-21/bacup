Function Fragment_Stage_0020_Item_00()
    W05_Jen205_Script jenScript = Alias_Jen as W05_Jen205_Script
    If jenScript != None
        jenScript.ActivateStealth()
    EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_205P_Started, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
    SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0040_Item_00()
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
    If Alias_InitEnableMarker.GetReference() != None
        Alias_InitEnableMarker.GetReference().Enable()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)
    If W05_MQS_205P_005_MotherlodeScene != None
        W05_MQS_205P_005_MotherlodeScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
    If GetStage() < 300
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    If TunnelScene != None
        TunnelScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
    If W05_MQS_205P_003_InTunnelScene != None
        W05_MQS_205P_003_InTunnelScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    If W05_MQS_205P_004_MoleMinerScene != None
        W05_MQS_205P_004_MoleMinerScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0450_Item_00()
    If W05_MQS_205P_450_Railroad != None
        W05_MQS_205P_450_Railroad.Start()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(40)
    If W05_MQS_205P_006_MotherlodeSpeaks != None && !W05_MQS_205P_006_MotherlodeSpeaks.IsPlaying()
        W05_MQS_205P_006_MotherlodeSpeaks.Start()
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(50)
    If W05_MQS_205P_007_MotherlodeDestroyed != None
        W05_MQS_205P_007_MotherlodeDestroyed.Start()
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(60)
    ObjectReference dirtMarker = Alias_DirtEnableMarker.GetReference()
    If dirtMarker != None
        dirtMarker.Enable()
    EndIf
    ObjectReference stagingDoor = Alias_RobotStagingDoor.GetReference()
    If stagingDoor != None
        stagingDoor.SetOpen(True)
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(65)
    If W05_MQS_205P_009_AlarmScene != None
        W05_MQS_205P_009_AlarmScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0950_Item_00()
    SetObjectiveCompleted(65)
    SetObjectiveDisplayed(70)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(70)
    SetObjectiveDisplayed(80)
EndFunction

Function Fragment_Stage_1050_Item_00()
    SetObjectiveCompleted(80)
    SetObjectiveDisplayed(90)
EndFunction

Function Fragment_Stage_1100_Item_00()
    ; Sibling script on the same QUST form: reach it through the shared Quest base.
    Quest owningQuest = Self as Quest
    DefaultQuestEncounterWaveScript waveController = owningQuest as DefaultQuestEncounterWaveScript
    If waveController != None
        waveController.StartLocalEncounterWave(0)
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    SetObjectiveCompleted(90)
    If W05_MQS_205P_011_RobotsDeadScene != None
        W05_MQS_205P_011_RobotsDeadScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1400_Item_00()
    ObjectReference gridRef = Alias_LGcollisionBox.GetReference()
    If gridRef != None
        gridRef.Disable()
    EndIf
    gridRef = Alias_LGcollisionBox02.GetReference()
    If gridRef != None
        gridRef.Disable()
    EndIf
    gridRef = Alias_LaserGrid01.GetReference()
    If gridRef != None
        gridRef.Disable()
    EndIf
    gridRef = Alias_LaserGrid02.GetReference()
    If gridRef != None
        gridRef.Disable()
    EndIf
    gridRef = Alias_LaserGrid03.GetReference()
    If gridRef != None
        gridRef.Disable()
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_205P_LaserGridState, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_1600_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(120)
    Actor pennyRef = Alias_PennyHornwright.GetActorReference()
    If pennyRef != None
        pennyRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1800_Item_00()
    If W05_MQS_205P_014_LaserTurretScene != None && !W05_MQS_205P_014_LaserTurretScene.IsPlaying()
        W05_MQS_205P_014_LaserTurretScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1900_Item_00()
    SetObjectiveCompleted(120)
    SetObjectiveDisplayed(130)
    If W05_MQS_205P_016A_ToolsScene != None
        W05_MQS_205P_016A_ToolsScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1950_Item_00()
    If W05_MQS_205P_LastTurrets != None && !W05_MQS_205P_LastTurrets.IsPlaying()
        W05_MQS_205P_LastTurrets.Start()
    EndIf
EndFunction

Function Fragment_Stage_2000_Item_00()
    SetObjectiveCompleted(130)
    SetObjectiveDisplayed(140)
    ObjectReference enableMarker = Alias_EnableRobotsMarker.GetReference()
    If enableMarker != None
        enableMarker.Enable()
    EndIf
EndFunction

Function Fragment_Stage_2100_Item_00()
    SetObjectiveCompleted(140)
    SetObjectiveDisplayed(150)
    If W05_MQS_205P_017_AtriumScene != None
        W05_MQS_205P_017_AtriumScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_2200_Item_00()
    SetObjectiveCompleted(150)
    ObjectReference doorRef = Alias_AtriumExit.GetReference()
    If doorRef != None
        doorRef.SetOpen(True)
    EndIf
    doorRef = Alias_endDoor.GetReference()
    If doorRef != None
        doorRef.SetOpen(True)
    EndIf
EndFunction

Function Fragment_Stage_2300_Item_00()
    If W05_MQS_205P_QuestEndScene != None
        W05_MQS_205P_QuestEndScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_PaigeIsInFoundation, 1.0)
        playerRef.SetValue(W05_PennyIsInFoundation, 1.0)
        playerRef.SetValue(W05_JenIsInFoundation, 1.0)
    EndIf
    If W05_MQA_206P_QuestStart_Keyword != None
        W05_MQA_206P_QuestStart_Keyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction

Function Fragment_Stage_10000_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveCompleted(20)
    SetObjectiveCompleted(30)
    SetObjectiveCompleted(40)
    SetObjectiveCompleted(50)
    SetObjectiveCompleted(60)
    SetObjectiveCompleted(65)
    SetObjectiveCompleted(70)
    SetObjectiveCompleted(80)
    SetObjectiveCompleted(90)
    SetObjectiveCompleted(100)
    SetObjectiveCompleted(120)
    SetObjectiveCompleted(130)
    SetObjectiveCompleted(140)
    SetObjectiveCompleted(150)
EndFunction
