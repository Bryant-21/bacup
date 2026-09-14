Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_203P_Started, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0301_Item_00()
    If W05_MQS_203P_005_RobcoEntrance != None && !W05_MQS_203P_005_RobcoEntrance.IsPlaying()
        W05_MQS_203P_005_RobcoEntrance.Start()
    EndIf
EndFunction

Function Fragment_Stage_0310_Item_00()
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0400_Item_00()
    If W05_MQS_203P_006_RobCoFacilityScene != None
        W05_MQS_203P_006_RobCoFacilityScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(60)
    If W05_MQS_203P_007_DiscoverRobobrainScene != None
        W05_MQS_203P_007_DiscoverRobobrainScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(70)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_203P_CanInteractBrokenRobo, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    ; The three brain-jar activators and their jar meshes are enable-parented to
    ; the BrainStorage enable markers, which ship initially disabled. Nothing else
    ; enables them, so stages 710/720/730 are unreachable until they are enabled.
    ObjectReference storageMarker = Alias_EnableMarker_BrainStorage_Dias.GetReference()
    If storageMarker != None
        storageMarker.Enable()
    EndIf
    storageMarker = Alias_EnableMarker_BrainStorage_Greg.GetReference()
    If storageMarker != None
        storageMarker.Enable()
    EndIf
    storageMarker = Alias_EnableMarker_BrainStorage_Gina.GetReference()
    If storageMarker != None
        storageMarker.Enable()
    EndIf
    If W05_MQS_203P_009_EnterBrainRoom != None
        W05_MQS_203P_009_EnterBrainRoom.Start()
    EndIf
EndFunction

Function Fragment_Stage_0710_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_203P_HasDiasBrain, 1.0)
        If playerRef.GetItemCount(W05_MQS_203P_BrainJar_Dias) < 1
            playerRef.AddItem(W05_MQS_203P_BrainJar_Dias, 1, False)
        EndIf
    EndIf
    If !IsStageDone(800)
        SetStage(800)
    EndIf
EndFunction

Function Fragment_Stage_0720_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_203P_HasGregBrain, 1.0)
        If playerRef.GetItemCount(W05_MQS_203P_BrainJar_Greg) < 1
            playerRef.AddItem(W05_MQS_203P_BrainJar_Greg, 1, False)
        EndIf
    EndIf
    If !IsStageDone(800)
        SetStage(800)
    EndIf
EndFunction

Function Fragment_Stage_0730_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_203P_HasGinaBrain, 1.0)
        If playerRef.GetItemCount(W05_MQS_203P_BrainJar_Gina) < 1
            playerRef.AddItem(W05_MQS_203P_BrainJar_Gina, 1, False)
        EndIf
    EndIf
    If !IsStageDone(800)
        SetStage(800)
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(70)
    SetObjectiveDisplayed(80)
    SetObjectiveDisplayed(85, True, False)
    If W05_MQS_203P_008_BrainScene != None && !W05_MQS_203P_008_BrainScene.IsPlaying()
        W05_MQS_203P_008_BrainScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0801_Item_00()
    ObjectReference noteMarker = Alias_EnableMarker_Notes.GetReference()
    If noteMarker != None
        noteMarker.Enable()
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    If W05_MQS_203P_010_EnterPrepScene != None
        W05_MQS_203P_010_EnterPrepScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0905_Item_00()
    SetObjectiveDisplayed(85, True, False)
EndFunction

Function Fragment_Stage_0910_Item_00()
    SetObjectiveDisplayed(85, True, False)
EndFunction

Function Fragment_Stage_0920_Item_00()
    SetObjectiveDisplayed(81, True, False)
EndFunction

Function Fragment_Stage_0921_Item_00()
    SetObjectiveCompleted(81)
EndFunction

Function Fragment_Stage_0930_Item_00()
    SetObjectiveDisplayed(82, True, False)
EndFunction

Function Fragment_Stage_0931_Item_00()
    SetObjectiveCompleted(82)
EndFunction

Function Fragment_Stage_0940_Item_00()
    SetObjectiveDisplayed(83, True, False)
EndFunction

Function Fragment_Stage_0941_Item_00()
    SetObjectiveCompleted(83)
EndFunction

Function Fragment_Stage_0950_Item_00()
    SetObjectiveCompleted(85)
    SetObjectiveDisplayed(84)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_203P_CanInteractBrainPrep, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(80)
    SetObjectiveCompleted(84)
    SetObjectiveDisplayed(90)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_203P_CanInteractBrainPrep, 0.0)
    EndIf
EndFunction

Function Fragment_Stage_1001_Item_00()
    DefaultQuestEncounterWaveScript waveController = (Self as Quest) as DefaultQuestEncounterWaveScript
    If waveController != None
        waveController.StartLocalEncounterWave(0)
    EndIf
EndFunction

Function Fragment_Stage_1002_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.RemoveItem(W05_MQS_203P_BrainJar_Dias, 1, True)
        If playerRef.GetItemCount(W05_MQS_203P_BrainJarPrepped_Dias) < 1
            playerRef.AddItem(W05_MQS_203P_BrainJarPrepped_Dias, 1, False)
        EndIf
        playerRef.SetValue(W05_MQS_203P_HasPreppedDiasBrain, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1003_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.RemoveItem(W05_MQS_203P_BrainJar_Greg, 1, True)
        If playerRef.GetItemCount(W05_MQS_203P_BrainJarPrepped_Greg) < 1
            playerRef.AddItem(W05_MQS_203P_BrainJarPrepped_Greg, 1, False)
        EndIf
        playerRef.SetValue(W05_MQS_203P_HasPreppedGregBrain, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1004_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.RemoveItem(W05_MQS_203P_BrainJar_Gina, 1, True)
        If playerRef.GetItemCount(W05_MQS_203P_BrainJarPrepped_Gina) < 1
            playerRef.AddItem(W05_MQS_203P_BrainJarPrepped_Gina, 1, False)
        EndIf
        playerRef.SetValue(W05_MQS_203P_HasPreppedGinaBrain, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1005_Item_00()
    ObjectReference domeMarker = Alias_EnableMarker_RobobrainDome.GetReference()
    If domeMarker != None
        domeMarker.Enable()
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    If W05_MQS_203P_013_EnterStorageScene != None && !W05_MQS_203P_013_EnterStorageScene.IsPlaying()
        W05_MQS_203P_013_EnterStorageScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveCompleted(90)
    SetObjectiveDisplayed(100)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && playerRef.GetItemCount(W05_MQS_203P_RobobrainDome) < 1
        playerRef.AddItem(W05_MQS_203P_RobobrainDome, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    Actor playerRef = Game.GetPlayer()
    Bool assembledBrain = False
    If playerRef != None && playerRef.GetItemCount(W05_MQS_203P_RobobrainDome) > 0
        If playerRef.GetValue(W05_MQS_203P_ChoseDias) > 0.0 && playerRef.GetItemCount(W05_MQS_203P_BrainJarPrepped_Dias) > 0
            playerRef.RemoveItem(W05_MQS_203P_BrainJarPrepped_Dias, 1, True)
            assembledBrain = True
        ElseIf playerRef.GetValue(W05_MQS_203P_ChoseGreg) > 0.0 && playerRef.GetItemCount(W05_MQS_203P_BrainJarPrepped_Greg) > 0
            playerRef.RemoveItem(W05_MQS_203P_BrainJarPrepped_Greg, 1, True)
            assembledBrain = True
        ElseIf playerRef.GetValue(W05_MQS_203P_ChoseGina) > 0.0 && playerRef.GetItemCount(W05_MQS_203P_BrainJarPrepped_Gina) > 0
            playerRef.RemoveItem(W05_MQS_203P_BrainJarPrepped_Gina, 1, True)
            assembledBrain = True
        EndIf
        If assembledBrain
            playerRef.RemoveItem(W05_MQS_203P_RobobrainDome, 1, True)
        EndIf
    EndIf
    If assembledBrain
        SetObjectiveCompleted(100)
        SetObjectiveDisplayed(110)
    EndIf
EndFunction

Function Fragment_Stage_1400_Item_00()
    SetObjectiveCompleted(110)
    SetObjectiveDisplayed(120)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        If playerRef.GetValue(W05_MQS_203P_ChoseDias) > 0.0
            W05_MQS_203P_015A_DoctorDiasMakesTools.Start()
        ElseIf playerRef.GetValue(W05_MQS_203P_ChoseGreg) > 0.0
            W05_MQS_203P_015C_GregMakesTools.Start()
        ElseIf playerRef.GetValue(W05_MQS_203P_ChoseGina) > 0.0
            W05_MQS_203P_015B_GinaMakesTools.Start()
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
    SetObjectiveCompleted(120)
    SetObjectiveDisplayed(130)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        If playerRef.GetValue(W05_MQS_203P_ChoseDias) > 0.0 && !IsStageDone(1510)
            SetStage(1510)
        ElseIf playerRef.GetValue(W05_MQS_203P_ChoseGreg) > 0.0 && !IsStageDone(1520)
            SetStage(1520)
        ElseIf playerRef.GetValue(W05_MQS_203P_ChoseGina) > 0.0 && !IsStageDone(1530)
            SetStage(1530)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1501_Item_00()
    SetObjectiveDisplayed(130)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        If playerRef.GetValue(W05_MQS_203P_ChoseDias) > 0.0 && !IsStageDone(1510)
            SetStage(1510)
        ElseIf playerRef.GetValue(W05_MQS_203P_ChoseGreg) > 0.0 && !IsStageDone(1520)
            SetStage(1520)
        ElseIf playerRef.GetValue(W05_MQS_203P_ChoseGina) > 0.0 && !IsStageDone(1530)
            SetStage(1530)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1510_Item_00()
    SetObjectiveDisplayed(130)
    ObjectReference markerRef = Alias_EnableMarker_Tools_DiasVolatile.GetReference()
    If markerRef != None
        markerRef.Enable()
    EndIf
EndFunction

Function Fragment_Stage_1520_Item_00()
    SetObjectiveDisplayed(130)
    ObjectReference markerRef = Alias_EnableMarker_Tools_GregStandard.GetReference()
    If markerRef != None
        markerRef.Enable()
    EndIf
EndFunction

Function Fragment_Stage_1530_Item_00()
    SetObjectiveDisplayed(130)
    ObjectReference markerRef = Alias_EnableMarker_Tools_GinaClever.GetReference()
    If markerRef != None
        markerRef.Enable()
    EndIf
EndFunction

Function Fragment_Stage_1600_Item_00()
    SetObjectiveCompleted(130)
    SetObjectiveDisplayed(140)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        If playerRef.GetValue(W05_MQS_203P_ChoseDias) > 0.0
            playerRef.SetValue(W05_MQS_203P_HasToolsVolatile, 1.0)
        ElseIf playerRef.GetValue(W05_MQS_203P_ChoseGreg) > 0.0
            playerRef.SetValue(W05_MQS_203P_HasToolsStandard, 1.0)
        ElseIf playerRef.GetValue(W05_MQS_203P_ChoseGina) > 0.0
            playerRef.SetValue(W05_MQS_203P_HasToolsClever, 1.0)
        EndIf
    EndIf
    If W05_MQS_203P_016_PickedUpToolsScene != None && !W05_MQS_203P_016_PickedUpToolsScene.IsPlaying()
        W05_MQS_203P_016_PickedUpToolsScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1700_Item_00()
    SetObjectiveCompleted(140)
    SetObjectiveDisplayed(150)
EndFunction

Function Fragment_Stage_1800_Item_00()
    SetObjectiveCompleted(150)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_2000_Item_00()
    DefaultQuestEncounterWaveScript waveController = (Self as Quest) as DefaultQuestEncounterWaveScript
    If waveController != None
        waveController.StartLocalEncounterWave(1)
    EndIf
EndFunction

Function Fragment_Stage_2100_Item_00()
    DefaultQuestEncounterWaveScript waveController = (Self as Quest) as DefaultQuestEncounterWaveScript
    If waveController != None
        waveController.StartLocalEncounterWave(2)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_203P_QuestComplete, 1.0)
        playerRef.SetValue(W05_RadcliffIsInFoundation, 1.0)
    EndIf
    If W05_MQS_Choice_QuestStartKeyword != None
        W05_MQS_Choice_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction

Function Fragment_Stage_9999_Item_00()
    Stop()
EndFunction

Function Fragment_Stage_10000_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveCompleted(20)
    SetObjectiveCompleted(30)
    SetObjectiveCompleted(40)
    SetObjectiveCompleted(50)
    SetObjectiveCompleted(60)
    SetObjectiveCompleted(70)
    SetObjectiveCompleted(80)
    SetObjectiveCompleted(84)
    SetObjectiveCompleted(85)
    SetObjectiveCompleted(90)
    SetObjectiveCompleted(100)
    SetObjectiveCompleted(110)
    SetObjectiveCompleted(120)
    SetObjectiveCompleted(129)
    SetObjectiveCompleted(130)
    SetObjectiveCompleted(140)
    SetObjectiveCompleted(150)
EndFunction
