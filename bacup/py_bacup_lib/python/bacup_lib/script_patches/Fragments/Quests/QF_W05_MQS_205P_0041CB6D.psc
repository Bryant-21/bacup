; TODO

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
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0200_Item_00()
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

Function Fragment_Stage_0700_Item_00()
    If W05_MQS_205P_007_MotherlodeDestroyed != None
        W05_MQS_205P_007_MotherlodeDestroyed.Start()
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    If W05_MQS_205P_009_AlarmScene != None
        W05_MQS_205P_009_AlarmScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    If W05_MQS_205P_008_LaserGridScene != None
        W05_MQS_205P_008_LaserGridScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1050_Item_00()
    If GetStage() < 1100
        SetStage(1100)
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    ObjectReference enableMarker = Alias_EnableRobotsMarker.GetReference()
    If enableMarker != None
        enableMarker.Enable()
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    If W05_MQS_205P_011_RobotsDeadScene != None
        W05_MQS_205P_011_RobotsDeadScene.Start()
    EndIf
    If GetStage() < 1400
        SetStage(1400)
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
    If GetStage() < 1500
        SetStage(1500)
    EndIf
EndFunction

Function Fragment_Stage_1900_Item_00()
    If W05_MQS_205P_016A_ToolsScene != None
        W05_MQS_205P_016A_ToolsScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_2000_Item_00()
    ObjectReference enableMarker = Alias_EnableRobotsMarker.GetReference()
    If enableMarker != None
        enableMarker.Enable()
    EndIf
EndFunction

Function Fragment_Stage_2100_Item_00()
    If W05_MQS_205P_017_AtriumScene != None
        W05_MQS_205P_017_AtriumScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_2200_Item_00()
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
