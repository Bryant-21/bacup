Function Fragment_Stage_0005_Item_00()
    If W05_MQ_101P_Started
        Game.GetPlayer().SetValue(W05_MQ_101P_Started, 1.0)
    EndIf
    If !IsStageDone(10) && !IsStageDone(20)
        SetStage(10)
    EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10)
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    W05_MQ_101P_Radio_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
EndFunction

Function Fragment_Stage_0013_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(13)
EndFunction

Function Fragment_Stage_0015_Item_00()
    SetObjectiveCompleted(13)
    SetObjectiveDisplayed(15)
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveCompleted(13)
    SetObjectiveCompleted(15)
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0030_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(22)
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    RS03_Inoculation_Keyword.SendStoryEvent(None, playerRef, playerRef)
EndFunction

Function Fragment_Stage_0040_Item_00()
    SetObjectiveCompleted(22)
    SetObjectiveDisplayed(25)
EndFunction

Function Fragment_Stage_0050_Item_00()
    Game.GetPlayer().SetValue(W05_MQ_101P_TrackCapsBonus_Meg, 2.0)
EndFunction

Function Fragment_Stage_0051_Item_00()
    Game.GetPlayer().SetValue(W05_MQ_101P_TrackCapsBonus_Meg, 1.0)
EndFunction

Function Fragment_Stage_0052_Item_00()
    Game.GetPlayer().SetValue(W05_MQ_101P_TrackCapsBonus_Paige, 1.0)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(14)
    SetObjectiveCompleted(25)
    SetObjectiveDisplayed(30)
    SetObjectiveDisplayed(40)
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If W05_MQ_101P_A && !W05_MQ_101P_A.IsRunning() && !W05_MQ_101P_A.IsCompleted()
        W05_MQ_101P_A_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
    If W05_MQ_101P_B && !W05_MQ_101P_B.IsRunning() && !W05_MQ_101P_B.IsCompleted()
        W05_MQ_101P_B_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0110_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(21)
    If W05_MQ_101P_000_HouseScene && !W05_MQ_101P_000_HouseScene.IsPlaying()
        W05_MQ_101P_000_HouseScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0120_Item_00()
    SetObjectiveCompleted(21)
    SetObjectiveDisplayed(22)
    ObjectReference homeDoor = Alias_OverseerHomeDoor01.GetReference()
    If homeDoor
        homeDoor.Unlock()
        homeDoor.SetOpen(True)
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveCompleted(21)
    SetObjectiveDisplayed(30)
    SetObjectiveDisplayed(40)
    If !IsStageDone(100)
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(30)
    If IsStageDone(300) && !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(40)
    If IsStageDone(200) && !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(50)
    Actor overseerRef = Alias_OverseerSutton.GetActorReference()
    If overseerRef
        overseerRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(60)
    Actor overseerRef = Alias_OverseerSutton.GetActorReference()
    If overseerRef
        overseerRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0550_Item_00()
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(70)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(70)
    If W05_MQ_101P_003_ColaPlantEntranceScene && !W05_MQ_101P_003_ColaPlantEntranceScene.IsPlaying()
        W05_MQ_101P_003_ColaPlantEntranceScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(70)
    SetObjectiveDisplayed(75)
    Actor overseerRef = Alias_OverseerColaPlant.GetActorReference()
    If overseerRef
        overseerRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(75)
    SetObjectiveDisplayed(80)
    If W05_MQ_101P_004_ReactorScene && !W05_MQ_101P_004_ReactorScene.IsPlaying()
        W05_MQ_101P_004_ReactorScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0805_Item_00()
    SetObjectiveDisplayed(80)
    ObjectReference coupling01 = Alias_PowerCoupling01.GetReference()
    ObjectReference coupling02 = Alias_PowerCoupling02.GetReference()
    ObjectReference coupling03 = Alias_PowerCoupling03.GetReference()
    If coupling01
        coupling01.Enable()
    EndIf
    If coupling02
        coupling02.Enable()
    EndIf
    If coupling03
        coupling03.Enable()
    EndIf
EndFunction

Function Fragment_Stage_0810_Item_00()
    If IsStageDone(820) && IsStageDone(830) && !IsStageDone(900)
        SetStage(900)
    EndIf
EndFunction

Function Fragment_Stage_0820_Item_00()
    If IsStageDone(810) && IsStageDone(830) && !IsStageDone(900)
        SetStage(900)
    EndIf
EndFunction

Function Fragment_Stage_0830_Item_00()
    If IsStageDone(810) && IsStageDone(820) && !IsStageDone(900)
        SetStage(900)
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(80)
    SetObjectiveDisplayed(90)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(75)
    SetObjectiveCompleted(80)
    SetObjectiveCompleted(90)
    If W05_MQ_101P_004B_ReactorStartScene && !W05_MQ_101P_004B_ReactorStartScene.IsPlaying()
        W05_MQ_101P_004B_ReactorStartScene.Start()
    EndIf
    If IsStageDone(1400) && !IsStageDone(1450)
        SetStage(1450)
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveDisplayed(100)
    If W05_MQ_101P_005_LabScene && !W05_MQ_101P_005_LabScene.IsPlaying()
        W05_MQ_101P_005_LabScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(110)
    If W05_MQ_101P_006_BloodScene && !W05_MQ_101P_006_BloodScene.IsPlaying()
        W05_MQ_101P_006_BloodScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1290_Item_00()
    SetObjectiveCompleted(110)
    SetObjectiveDisplayed(120)
    If W05_MQ_101P_007_FlavorSequencer && !W05_MQ_101P_007_FlavorSequencer.IsPlaying()
        W05_MQ_101P_007_FlavorSequencer.Start()
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    If W05_MQ_101P_005b_OverseerHelps && !W05_MQ_101P_005b_OverseerHelps.IsPlaying()
        W05_MQ_101P_005b_OverseerHelps.Start()
    EndIf
EndFunction

Function Fragment_Stage_1310_Item_00()
    DefaultQuestEncounterWaveScript encounterController = (Self as Quest) as DefaultQuestEncounterWaveScript
    If encounterController
        encounterController.StartLocalEncounterWave(0)
    ElseIf !IsStageDone(1400)
        SetStage(1400)
    EndIf
EndFunction

Function Fragment_Stage_1400_Item_00()
    SetObjectiveCompleted(120)
    If W05_MQ_101P_006_FlavorCompleted && !W05_MQ_101P_006_FlavorCompleted.IsPlaying()
        W05_MQ_101P_006_FlavorCompleted.Start()
    EndIf
    If IsStageDone(1000) && !IsStageDone(1450)
        SetStage(1450)
    EndIf
EndFunction

Function Fragment_Stage_1450_Item_00()
    SetObjectiveCompleted(75)
    SetObjectiveCompleted(120)
    SetObjectiveDisplayed(130)
    If W05_MQ_101P_09_BrandingComplete && !W05_MQ_101P_09_BrandingComplete.IsPlaying()
        W05_MQ_101P_09_BrandingComplete.Start()
    EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
    SetObjectiveCompleted(130)
    SetObjectiveDisplayed(140)
EndFunction

Function Fragment_Stage_1510_Item_00()
    SetObjectiveDisplayed(140)
    If W05_MQ_101P_010_AssemblyLineScene && !W05_MQ_101P_010_AssemblyLineScene.IsPlaying()
        W05_MQ_101P_010_AssemblyLineScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1600_Item_00()
    SetObjectiveCompleted(140)
    SetObjectiveDisplayed(150)
    ObjectReference conveyorRef = Alias_ConveyorBelt.GetReference()
    ObjectReference bottlesRef = Alias_ConveyorBottles.GetReference()
    ObjectReference audioRef = Alias_AudioConveyor.GetReference()
    If conveyorRef
        conveyorRef.Enable()
    EndIf
    If bottlesRef
        bottlesRef.Enable()
    EndIf
    If audioRef
        audioRef.Enable()
    EndIf
    If W05_MQ_101P_011_CasesScene && !W05_MQ_101P_011_CasesScene.IsPlaying()
        W05_MQ_101P_011_CasesScene.Start()
    EndIf
    If !IsStageDone(1700) && !IsStageDone(1610)
        SetStage(1610)
    EndIf
EndFunction

Function Fragment_Stage_1610_Item_00()
    SetObjectiveCompleted(150)
    SetObjectiveDisplayed(170)
    ObjectReference vaccineRef = NukaColaVaccine_QuestObject.GetReference()
    If vaccineRef
        vaccineRef.Enable()
    EndIf
EndFunction

Function Fragment_Stage_1700_Item_00()
    SetObjectiveCompleted(170)
    SetObjectiveDisplayed(180)
    SetObjectiveDisplayed(190)
EndFunction

Function Fragment_Stage_1800_Item_00()
    SetObjectiveCompleted(180)
    If IsStageDone(1900) && !IsStageDone(2000)
        SetStage(2000)
    EndIf
EndFunction

Function Fragment_Stage_1810_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef
        playerRef.AddItem(Caps001, 50, False)
    EndIf
EndFunction

Function Fragment_Stage_1820_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef
        playerRef.AddItem(Caps001, 50, False)
    EndIf
EndFunction

Function Fragment_Stage_1900_Item_00()
    SetObjectiveCompleted(190)
    If IsStageDone(1800) && !IsStageDone(2000)
        SetStage(2000)
    EndIf
EndFunction

Function Fragment_Stage_1910_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef
        playerRef.AddItem(Caps001, 50, False)
    EndIf
EndFunction

Function Fragment_Stage_1920_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef
        playerRef.AddItem(Caps001, 50, False)
    EndIf
EndFunction

Function Fragment_Stage_2000_Item_00()
    SetObjectiveCompleted(180)
    SetObjectiveCompleted(190)
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(200)
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    W05_MQ_102P_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
EndFunction
