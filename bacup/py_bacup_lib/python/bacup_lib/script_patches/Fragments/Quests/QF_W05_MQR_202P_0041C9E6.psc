Function Fragment_Stage_0001_Item_00()
    ; FO76 instanced-participant initialization has no local FO4 world mutation.
    Return
EndFunction

Function Fragment_Stage_0002_Item_00()
    ; FO76 instanced-participant shutdown has no local FO4 world mutation.
    Return
EndFunction

Function Fragment_Stage_0011_Item_00()
    ; FO76 participant bookkeeping has no single-player FO4 equivalent.
    Return
EndFunction

Function Fragment_Stage_0012_Item_00()
    ; FO76 participant bookkeeping has no single-player FO4 equivalent.
    Return
EndFunction

Function Fragment_Stage_0013_Item_00()
    ; FO76 participant bookkeeping has no single-player FO4 equivalent.
    Return
EndFunction

Function Fragment_Stage_0014_Item_00()
    ; FO76 participant bookkeeping has no single-player FO4 equivalent.
    Return
EndFunction

Function Fragment_Stage_0015_Item_00()
    ; FO76 participant bookkeeping has no single-player FO4 equivalent.
    Return
EndFunction

Function Fragment_Stage_0016_Item_00()
    ; FO76 participant bookkeeping has no single-player FO4 equivalent.
    Return
EndFunction

Function Fragment_Stage_0017_Item_00()
    ; FO76 participant bookkeeping has no single-player FO4 equivalent.
    Return
EndFunction

Function Fragment_Stage_0018_Item_00()
    ; FO76 participant bookkeeping has no single-player FO4 equivalent.
    Return
EndFunction

Function Fragment_Stage_0019_Item_00()
    ; FO76 participant bookkeeping has no single-player FO4 equivalent.
    Return
EndFunction

Function Fragment_Stage_0020_Item_00()
    ; FO76 participant bookkeeping has no single-player FO4 equivalent.
    Return
EndFunction

Function Fragment_Stage_0021_Item_00()
    ; FO76 participant bookkeeping has no single-player FO4 equivalent.
    Return
EndFunction

Function Fragment_Stage_0022_Item_00()
    ; FO76 participant bookkeeping has no single-player FO4 equivalent.
    Return
EndFunction

Function Fragment_Stage_0023_Item_00()
    ; FO76 participant bookkeeping has no single-player FO4 equivalent.
    Return
EndFunction

Function Fragment_Stage_0024_Item_00()
    ; FO76 participant bookkeeping has no single-player FO4 equivalent.
    Return
EndFunction

Function Fragment_Stage_0025_Item_00()
    ; FO76 participant bookkeeping has no single-player FO4 equivalent.
    Return
EndFunction

Function Fragment_Stage_0026_Item_00()
    ; FO76 participant bookkeeping has no single-player FO4 equivalent.
    Return
EndFunction

Function Fragment_Stage_0027_Item_00()
    ; FO76 participant bookkeeping has no single-player FO4 equivalent.
    Return
EndFunction

Function Fragment_Stage_0028_Item_00()
    ; FO76 participant bookkeeping has no single-player FO4 equivalent.
    Return
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0301_Item_00()
    If Alias_InitialRobots != None
        Alias_InitialRobots.EnableAll()
        If Alias_InitialRobots.GetCount() == 0 && !IsStageDone(310)
            SetStage(310)
        EndIf
    ElseIf !IsStageDone(310)
        SetStage(310)
    EndIf
EndFunction

Function Fragment_Stage_0310_Item_00()
    Actor raRaRef = Alias_RaRa.GetActorReference()

    SetObjectiveDisplayed(310)
    If raRaRef != None && W05_MQR_202P_RaRaVent_0310_ExitVent != None && !W05_MQR_202P_RaRaVent_0310_ExitVent.IsPlaying()
        W05_MQR_202P_RaRaVent_0310_ExitVent.Start()
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_0550_Item_00()
    ObjectReference sectorAlphaDoor

    If Alias_SectorAlphaDoor01 != None
        sectorAlphaDoor = Alias_SectorAlphaDoor01.GetReference()
    EndIf
    If sectorAlphaDoor != None
        sectorAlphaDoor.Lock(False)
        sectorAlphaDoor.SetOpen(True)
    EndIf
    If !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0605_Item_00()
    Actor raRaRef = Alias_RaRa.GetActorReference()

    If raRaRef != None
        raRaRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0610_Item_00()
    Actor raRaRef = Alias_RaRa.GetActorReference()

    If raRaRef != None
        raRaRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0620_Item_00()
    SetObjectiveDisplayed(620)
    If Alias_RobotsDoor01 == None || Alias_RobotsDoor01.GetCount() == 0
        If !IsStageDone(630)
            SetStage(630)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0630_Item_00()
    DefaultQuestEncounterWaveScript waveController = (Self as Quest) as DefaultQuestEncounterWaveScript

    If waveController != None
        waveController.StartLocalEncounterWave(0)
    EndIf
EndFunction

Function Fragment_Stage_0650_Item_00()
    ObjectReference sectorAlphaDoor

    If Alias_SectorAlphaDoor01 != None
        sectorAlphaDoor = Alias_SectorAlphaDoor01.GetReference()
    EndIf
    If sectorAlphaDoor != None
        sectorAlphaDoor.Lock(False)
        sectorAlphaDoor.SetOpen(True)
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveDisplayed(700)
EndFunction

Function Fragment_Stage_0750_Item_00()
    ObjectReference securityRoomDoor

    If Alias_SectorAlphaDoor02 != None
        securityRoomDoor = Alias_SectorAlphaDoor02.GetReference()
    EndIf
    If securityRoomDoor != None
        securityRoomDoor.Lock(False)
        securityRoomDoor.SetOpen(True)
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    Actor raRaRef = Alias_RaRa.GetActorReference()

    SetObjectiveDisplayed(800)
    If raRaRef != None && W05_MQR_202P_RaRaVent_0800_EnterAndExitVent != None && !W05_MQR_202P_RaRaVent_0800_EnterAndExitVent.IsPlaying()
        W05_MQR_202P_RaRaVent_0800_EnterAndExitVent.Start()
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveDisplayed(900)
EndFunction

Function Fragment_Stage_0910_Item_00()
    Actor raRaRef = Alias_RaRa.GetActorReference()

    If raRaRef != None
        raRaRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0920_Item_00()
    SetObjectiveDisplayed(920)
EndFunction

Function Fragment_Stage_0930_Item_00()
    If W05_MQR_202P_RaRa_004C_SnackEnd != None && !W05_MQR_202P_RaRa_004C_SnackEnd.IsPlaying()
        W05_MQR_202P_RaRa_004C_SnackEnd.Start()
    EndIf
EndFunction

Function Fragment_Stage_0940_Item_00()
    SetObjectiveDisplayed(940)
    If W05_MQR_202P_RaRa_004C_SnackEnd != None && !W05_MQR_202P_RaRa_004C_SnackEnd.IsPlaying()
        W05_MQR_202P_RaRa_004C_SnackEnd.Start()
    EndIf
EndFunction

Function Fragment_Stage_0970_Item_00()
    Actor raRaRef = Alias_RaRa.GetActorReference()

    SetObjectiveDisplayed(970)
    If raRaRef != None
        raRaRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveDisplayed(1000)
EndFunction

Function Fragment_Stage_1010_Item_00()
    Actor raRaRef = Alias_RaRa.GetActorReference()

    If raRaRef != None
        raRaRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1017_Item_00()
    ObjectReference powerArmor
    ObjectReference powerArmorHelmet

    If Alias_PowerArmor != None
        powerArmor = Alias_PowerArmor.GetReference()
    EndIf
    If Alias_PowerArmorHelmet != None
        powerArmorHelmet = Alias_PowerArmorHelmet.GetReference()
    EndIf
    If powerArmor != None && powerArmor.IsDisabled()
        powerArmor.Enable()
    EndIf
    If powerArmorHelmet != None && powerArmorHelmet.IsDisabled()
        powerArmorHelmet.Enable()
    EndIf
    If W05_MQR_202P_PA_SectorAlphaKeycard != None && !W05_MQR_202P_PA_SectorAlphaKeycard.IsPlaying()
        W05_MQR_202P_PA_SectorAlphaKeycard.Start()
    EndIf
EndFunction

Function Fragment_Stage_1020_Item_00()
    Actor raRaRef = Alias_RaRa.GetActorReference()

    If raRaRef != None
        raRaRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveDisplayed(1100)
EndFunction

Function Fragment_Stage_1150_Item_00()
    ObjectReference sectorBravoDoor

    If Alias_SectorBravoEntranceDoor != None
        sectorBravoDoor = Alias_SectorBravoEntranceDoor.GetReference()
    EndIf
    If sectorBravoDoor != None
        sectorBravoDoor.Lock(False)
        sectorBravoDoor.SetOpen(True)
    EndIf
EndFunction

Function Fragment_Stage_1210_Item_00()
    If Alias_Turrets01 != None
        Alias_Turrets01.EnableAll()
        If Alias_Turrets01.GetCount() == 0 && !IsStageDone(1215)
            SetStage(1215)
        EndIf
    ElseIf !IsStageDone(1215)
        SetStage(1215)
    EndIf
EndFunction

Function Fragment_Stage_1220_Item_00()
    If Alias_Turrets02 != None
        Alias_Turrets02.EnableAll()
        If Alias_Turrets02.GetCount() == 0 && !IsStageDone(1225)
            SetStage(1225)
        EndIf
    ElseIf !IsStageDone(1225)
        SetStage(1225)
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    Actor raRaRef = Alias_RaRa.GetActorReference()

    SetObjectiveDisplayed(1300)
    If raRaRef != None
        raRaRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1400_Item_00()
    SetObjectiveDisplayed(1400)
EndFunction

Function Fragment_Stage_1500_Item_00()
    Actor raRaRef = Alias_RaRa.GetActorReference()

    SetObjectiveDisplayed(1500)
    If raRaRef != None && W05_MQR_202P_RaRaVent_1500_PeekSequence != None && !W05_MQR_202P_RaRaVent_1500_PeekSequence.IsPlaying()
        W05_MQR_202P_RaRaVent_1500_PeekSequence.Start()
    EndIf
EndFunction

Function Fragment_Stage_1510_Item_00()
    SetObjectiveDisplayed(1510)
    If !IsStageDone(1520)
        SetStage(1520)
    EndIf
EndFunction

Function Fragment_Stage_1511_Item_00()
    If !IsStageDone(1520)
        SetStage(1520)
    EndIf
EndFunction

Function Fragment_Stage_1520_Item_00()
    ObjectReference robotsEnableMarker

    If Alias_SectorCharlieRobotsEnableMarker != None
        robotsEnableMarker = Alias_SectorCharlieRobotsEnableMarker.GetReference()
    EndIf
    If robotsEnableMarker != None
        robotsEnableMarker.Enable()
    EndIf
    If W05_MQR_202P_PA_SectorCharlieRobots != None && !W05_MQR_202P_PA_SectorCharlieRobots.IsPlaying()
        W05_MQR_202P_PA_SectorCharlieRobots.Start()
    EndIf
    If Alias_RobotsSectorCharlie == None || Alias_RobotsSectorCharlie.GetCount() == 0
        If !IsStageDone(1530)
            SetStage(1530)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1530_Item_00()
    ObjectReference sectorCharlieDoor

    If Alias_SectorCharlieDoor != None
        sectorCharlieDoor = Alias_SectorCharlieDoor.GetReference()
    EndIf
    If sectorCharlieDoor != None
        sectorCharlieDoor.Lock(False)
        sectorCharlieDoor.SetOpen(True)
    EndIf
EndFunction

Function Fragment_Stage_1600_Item_00()
    DefaultQuestEncounterWaveScript waveController = (Self as Quest) as DefaultQuestEncounterWaveScript
    W05_MQR_202P_QuestScript bossVentController = (Self as Quest) as W05_MQR_202P_QuestScript

    SetObjectiveDisplayed(1600)
    If waveController != None
        waveController.StartLocalEncounterWave(1)
    EndIf
    If bossVentController != None
        bossVentController.StartBossVentCycle()
    EndIf
EndFunction

Function Fragment_Stage_1610_Item_00()
    SetObjectiveDisplayed(1610)
EndFunction

Function Fragment_Stage_1650_Item_00()
    Actor raRaRef = Alias_RaRa.GetActorReference()

    If raRaRef != None && W05_MQR_202P_RaRaVent_1650_ExitVent != None && !W05_MQR_202P_RaRaVent_1650_ExitVent.IsPlaying()
        W05_MQR_202P_RaRaVent_1650_ExitVent.Start()
    EndIf
EndFunction

Function Fragment_Stage_1700_Item_00()
    SetObjectiveDisplayed(1700)
EndFunction

Function Fragment_Stage_1800_Item_00()
    SetObjectiveDisplayed(1800)
EndFunction

Function Fragment_Stage_1810_Item_00()
    ; FO76 account reputation mutation is intentionally omitted.
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_1811_Item_00()
    ; FO76 account reputation mutation is intentionally omitted.
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    ObjectReference playerRef

    If Alias_currentPlayer != None
        playerRef = Alias_currentPlayer.GetReference()
    EndIf
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If W05_MQR_203P_QuestStart_Keyword != None && playerRef != None
        W05_MQR_203P_QuestStart_Keyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction

Function Fragment_Stage_9999_Item_00()
    Stop()
EndFunction
