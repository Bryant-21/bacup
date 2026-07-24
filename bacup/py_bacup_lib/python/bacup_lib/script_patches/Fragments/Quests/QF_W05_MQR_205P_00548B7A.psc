; TODO

Function Fragment_Stage_0001_Item_00()
    ObjectReference initMarker = Alias_InitEnableMarker.GetReference()
    If initMarker != None
        initMarker.Enable()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0105_Item_00()
    SetObjectiveDisplayed(105)
EndFunction

Function Fragment_Stage_0106_Item_00()
    SetObjectiveCompleted(105)
EndFunction

Function Fragment_Stage_0110_Item_00()
    If W05_MQR_205P_001_IntroScene != None && !W05_MQR_205P_001_IntroScene.IsPlaying()
        W05_MQR_205P_001_IntroScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0210_Item_00()
    SetObjectiveDisplayed(210)
    If W05_MQR_205P_002_Lou_Door02 != None && !W05_MQR_205P_002_Lou_Door02.IsPlaying()
        W05_MQR_205P_002_Lou_Door02.Start()
    EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
    SetObjectiveCompleted(210)
EndFunction

Function Fragment_Stage_0260_Item_00()
    If W05_MQR_205P_003_DoorBlownUp != None && !W05_MQR_205P_003_DoorBlownUp.IsPlaying()
        W05_MQR_205P_003_DoorBlownUp.Start()
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveDisplayed(300)
    If W05_MQR_205P_0300_EnterVault != None && !W05_MQR_205P_0300_EnterVault.IsPlaying()
        W05_MQR_205P_0300_EnterVault.Start()
    EndIf
EndFunction

Function Fragment_Stage_0305_Item_00()
    If W05_MQR_205P_004A_Meg_ComeBack != None && !W05_MQR_205P_004A_Meg_ComeBack.IsPlaying()
        W05_MQR_205P_004A_Meg_ComeBack.Start()
    EndIf
EndFunction

Function Fragment_Stage_0310_Item_00()
    SetObjectiveDisplayed(310)
EndFunction

Function Fragment_Stage_0315_Item_00()
    If W05_MQR_205P_004A_JohnnyDoor != None && !W05_MQR_205P_004A_JohnnyDoor.IsPlaying()
        W05_MQR_205P_004A_JohnnyDoor.Start()
    EndIf
EndFunction

Function Fragment_Stage_0320_Item_00()
    SetObjectiveDisplayed(320)
EndFunction

Function Fragment_Stage_0325_Item_00()
    SetObjectiveDisplayed(325)
EndFunction

Function Fragment_Stage_0330_Item_00()
    SetObjectiveDisplayed(330)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveDisplayed(400)
    If W05_MQR_205P_005_SecurityRoom != None && !W05_MQR_205P_005_SecurityRoom.IsPlaying()
        W05_MQR_205P_005_SecurityRoom.Start()
    EndIf
EndFunction

Function Fragment_Stage_0410_Item_00()
    ObjectReference securityDoor = Alias_SecurityRoomDoor.GetReference()
    If securityDoor != None
        securityDoor.SetOpen(False)
    EndIf
    ObjectReference securityCollision = Alias_SecurityRoomCollision.GetReference()
    If securityCollision != None
        securityCollision.Enable()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_0550_Item_00()
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_0560_Item_00()
    SetObjectiveCompleted(500)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0610_Item_00()
    Actor gailRef = Alias_Gail.GetActorReference()
    If gailRef != None
        gailRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0615_Item_00()
    SetObjectiveCompleted(600)
    If !IsStageDone(620)
        SetStage(620)
    EndIf
EndFunction

Function Fragment_Stage_0620_Item_00()
    If !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveDisplayed(700)
EndFunction

Function Fragment_Stage_0710_Item_00()
    Actor johnnyRef = Alias_Johnny.GetActorReference()
    If johnnyRef != None
        johnnyRef.EvaluatePackage()
    EndIf
    Actor gailRef = Alias_Gail.GetActorReference()
    If gailRef != None
        gailRef.EvaluatePackage()
    EndIf
    Actor raRaRef = Alias_RaRa.GetActorReference()
    If raRaRef != None
        raRaRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveDisplayed(800)
EndFunction

Function Fragment_Stage_0810_Item_00()
    SetObjectiveDisplayed(810)
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveDisplayed(900)
EndFunction

Function Fragment_Stage_0905_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef != None
        playerRef.SetValue(W05_MQR_205P_RaRaOpenDoorsValue, 1.0)
    EndIf
    If !IsStageDone(910)
        SetStage(910)
    EndIf
EndFunction

Function Fragment_Stage_0906_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef != None
        playerRef.SetValue(W05_MQR_205P_RaRaOpenDoorsValue, 0.0)
    EndIf
    If !IsStageDone(910)
        SetStage(910)
    EndIf
EndFunction

Function Fragment_Stage_0910_Item_00()
    W05_MQR_205P_QuestScript owningQuestScript = Self as W05_MQR_205P_QuestScript
    If owningQuestScript != None && owningQuestScript.W05_MQR_205P_014_RaRa_OverseerVent03 != None && !owningQuestScript.W05_MQR_205P_014_RaRa_OverseerVent03.IsPlaying()
        owningQuestScript.W05_MQR_205P_014_RaRa_OverseerVent03.Start()
    EndIf
EndFunction

Function Fragment_Stage_0915_Item_00()
    If Alias_AtriumRobotsWave01 != None
        Alias_AtriumRobotsWave01.DisableAll()
    EndIf
    If !IsStageDone(920)
        SetStage(920)
    EndIf
EndFunction

Function Fragment_Stage_0920_Item_00()
    If W05_MQR_205P_015_RaRa_OverseerRoom != None && !W05_MQR_205P_015_RaRa_OverseerRoom.IsPlaying()
        W05_MQR_205P_015_RaRa_OverseerRoom.Start()
    EndIf
EndFunction

Function Fragment_Stage_0921_Item_00()
    ObjectReference exitDoor = Alias_OverseerRoomExitDoor.GetReference()
    If exitDoor != None
        exitDoor.Lock(False)
        exitDoor.SetOpen(True)
    EndIf
    If !IsStageDone(930)
        SetStage(930)
    EndIf
EndFunction

Function Fragment_Stage_0930_Item_00()
    SetObjectiveDisplayed(1000)
EndFunction

Function Fragment_Stage_0930_Item_01()
    If Alias_AtriumRobotsWave01 != None
        Alias_AtriumRobotsWave01.DisableAll()
    EndIf
EndFunction

Function Fragment_Stage_0940_Item_00()
    SetObjectiveDisplayed(1000)
EndFunction

Function Fragment_Stage_0940_Item_01()
    If Alias_AtriumRobotsWave01 != None
        Alias_AtriumRobotsWave01.DisableAll()
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveDisplayed(1100)
EndFunction

Function Fragment_Stage_1105_Item_00()
    Actor raRaRef = Alias_RaRa.GetActorReference()
    If raRaRef != None
        raRaRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1106_Item_00()
    Actor raRaRef = Alias_RaRa.GetActorReference()
    If raRaRef != None
        raRaRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1107_Item_00()
    If W05_MQR_205P_016B_RaRa_OptionalVent != None && !W05_MQR_205P_016B_RaRa_OptionalVent.IsPlaying()
        W05_MQR_205P_016B_RaRa_OptionalVent.Start()
    EndIf
EndFunction

Function Fragment_Stage_1110_Item_00()
    W05_MQR_205P_QuestScript owningQuestScript = Self as W05_MQR_205P_QuestScript
    If owningQuestScript != None && owningQuestScript.OptionalDoor != None
        ObjectReference optionalDoor = owningQuestScript.OptionalDoor.GetReference()
        If optionalDoor != None
            optionalDoor.Lock(False)
            optionalDoor.SetOpen(True)
        EndIf
    EndIf
    Actor raRaRef = Alias_RaRa.GetActorReference()
    If raRaRef != None
        raRaRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1111_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef != None
        playerRef.SetValue(W05_MQR_205P_PlasmaGunAcquiredValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1120_Item_00()
    Actor raRaRef = Alias_RaRa.GetActorReference()
    If raRaRef != None
        raRaRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveDisplayed(1200)
EndFunction

Function Fragment_Stage_1210_Item_00()
    If W05_MQR_205P_017_RaRa_LastVent02 != None && !W05_MQR_205P_017_RaRa_LastVent02.IsPlaying()
        W05_MQR_205P_017_RaRa_LastVent02.Start()
    EndIf
EndFunction

Function Fragment_Stage_1220_Item_00()
    Actor raRaRef = Alias_RaRa.GetActorReference()
    ObjectReference exitMarker = Alias_RaRaEndDoorVentExitMarker.GetReference()
    If raRaRef != None && exitMarker != None
        raRaRef.MoveTo(exitMarker)
        raRaRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1230_Item_00()
    ObjectReference endDoor = Alias_endDoor.GetReference()
    If endDoor != None
        endDoor.Lock(False)
        endDoor.SetOpen(True)
    EndIf
EndFunction

Function Fragment_Stage_1240_Item_00()
    Actor johnnyRef = Alias_Johnny.GetActorReference()
    If johnnyRef != None
        johnnyRef.EvaluatePackage()
    EndIf
    Actor gailRef = Alias_Gail.GetActorReference()
    If gailRef != None
        gailRef.EvaluatePackage()
    EndIf
    Actor raRaRef = Alias_RaRa.GetActorReference()
    If raRaRef != None
        raRaRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1250_Item_00()
    SetObjectiveCompleted(1200)
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If W05_MQA_206P_QuestStart_Keyword != None
        W05_MQA_206P_QuestStart_Keyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction
