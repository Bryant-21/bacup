; TODO

Function Fragment_Stage_0002_Item_00()
    ObjectReference disabledMarker = Alias_DisabledForQuestEnableMarker.GetReference()
    ObjectReference enabledMarker = Alias_EnabledForQuestEnableMarker.GetReference()
    ObjectReference entranceMarker = ArenaEntranceMarker.GetReference()
    ObjectReference entranceDoor = Alias_EntranceDoor.GetReference()
    If disabledMarker != None && enabledMarker != None && entranceMarker != None && entranceDoor != None
        disabledMarker.Disable()
        enabledMarker.Enable()
        entranceMarker.Enable()
        entranceDoor.Enable()
        If GetStage() < 300
            SetStage(300)
        EndIf
    EndIf
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

Function Fragment_Stage_0400_Item_00()
    SetObjectiveDisplayed(400)
    If GetStage() < 405
        SetStage(405)
    EndIf
EndFunction

Function Fragment_Stage_0405_Item_00()
    If W05_MQR_203P_Johnny_002B_RegistrationScene != None
        W05_MQR_203P_Johnny_002B_RegistrationScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0499_Item_00()
    If W05_MQR_203P_Johnny_002C_RegistrationScene != None
        W05_MQR_203P_Johnny_002C_RegistrationScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_0550_Item_00()
    If W05_MQR_203P_SargentoPA_002_Round01GhouldenBoy != None
        W05_MQR_203P_SargentoPA_002_Round01GhouldenBoy.Start()
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0605_Item_00()
    If W05_MQR_203P_SargentoPA_003_Round01CallPlayer != None
        W05_MQR_203P_SargentoPA_003_Round01CallPlayer.Start()
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveDisplayed(700)
EndFunction

Function Fragment_Stage_0710_Item_00()
    ObjectReference arenaDoor = Alias_EntranceDoor.GetReference()
    If arenaDoor != None
        arenaDoor.SetOpen(True)
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveDisplayed(800)
EndFunction

Function Fragment_Stage_0800_Item_01()
    If !IsStageDone(810)
        SetStage(810)
    EndIf
EndFunction

Function Fragment_Stage_0810_Item_00()
    If W05_MQR_203P_SargentoPA_004B_Round01End != None
        W05_MQR_203P_SargentoPA_004B_Round01End.Start()
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveDisplayed(900)
    If W05_MQR_203P_Johnny_EnterLockerRoom != None
        W05_MQR_203P_Johnny_EnterLockerRoom.Start()
    EndIf
EndFunction

Function Fragment_Stage_0910_Item_00()
    If !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_0950_Item_00()
    SetObjectiveDisplayed(950)
    If !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveDisplayed(1000)
    If !IsStageDone(1050)
        SetStage(1050)
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveDisplayed(1100)
    If W05_MQR_203P_SargentoPA_005_Round02CallPlayer != None
        W05_MQR_203P_SargentoPA_005_Round02CallPlayer.Start()
    EndIf
EndFunction

Function Fragment_Stage_1110_Item_00()
    ObjectReference arenaDoor = Alias_EntranceDoor.GetReference()
    If arenaDoor != None
        arenaDoor.SetOpen(True)
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveDisplayed(1200)
EndFunction

Function Fragment_Stage_1200_Item_01()
    If !IsStageDone(1210)
        SetStage(1210)
    EndIf
EndFunction

Function Fragment_Stage_1210_Item_00()
    If W05_MQR_203P_SargentoPA_006B_Round02End != None
        W05_MQR_203P_SargentoPA_006B_Round02End.Start()
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    SetObjectiveDisplayed(1300)
    If W05_MQR_203P_Johnny_EnterLockerRoom != None
        W05_MQR_203P_Johnny_EnterLockerRoom.Start()
    EndIf
EndFunction

Function Fragment_Stage_1310_Item_00()
    If !IsStageDone(1400)
        SetStage(1400)
    EndIf
EndFunction

Function Fragment_Stage_1350_Item_00()
    SetObjectiveDisplayed(1350)
    If !IsStageDone(1400)
        SetStage(1400)
    EndIf
EndFunction

Function Fragment_Stage_1400_Item_00()
    SetObjectiveDisplayed(1400)
    If !IsStageDone(1450)
        SetStage(1450)
    EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
    SetObjectiveDisplayed(1500)
    If W05_MQR_203P_SargentoPA_007_Round03CallPlayer != None
        W05_MQR_203P_SargentoPA_007_Round03CallPlayer.Start()
    EndIf
EndFunction

Function Fragment_Stage_1510_Item_00()
    ObjectReference arenaDoor = Alias_EntranceDoor.GetReference()
    If arenaDoor != None
        arenaDoor.SetOpen(True)
    EndIf
EndFunction

Function Fragment_Stage_1600_Item_00()
    SetObjectiveDisplayed(1600)
    If W05_MQR_203P_SargentoPA_008_Round03PlayerArena != None
        W05_MQR_203P_SargentoPA_008_Round03PlayerArena.Start()
    EndIf
EndFunction

Function Fragment_Stage_1601_Item_00()
    ObjectReference cageActivator = GraftonCageSequenceActivator.GetReference()
    Actor playerRef = Game.GetPlayer()
    If cageActivator != None && playerRef != None
        cageActivator.Activate(playerRef)
        If !IsStageDone(1602)
            SetStage(1602)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1610_Item_00()
    If !IsStageDone(1700)
        SetStage(1700)
    EndIf
EndFunction

Function Fragment_Stage_1700_Item_00()
    SetObjectiveDisplayed(1700)
    If W05_MQR_203P_SargentoPA_009A_Winner != None
        W05_MQR_203P_SargentoPA_009A_Winner.Start()
    EndIf
EndFunction

Function Fragment_Stage_1705_Item_00()
    If !IsStageDone(1715)
        SetStage(1715)
    EndIf
EndFunction

Function Fragment_Stage_1715_Item_00()
    If W05_MQR_203P_Johnny_006_EnterArena != None
        W05_MQR_203P_Johnny_006_EnterArena.Start()
    EndIf
EndFunction

Function Fragment_Stage_1720_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && playerRef.GetItemCount(W05_MQR_203P_HalRoomKey) < 1
        playerRef.AddItem(W05_MQR_203P_HalRoomKey, 1, False)
    EndIf
    If !IsStageDone(1750)
        SetStage(1750)
    EndIf
EndFunction

Function Fragment_Stage_1750_Item_00()
    If W05_MQR_203P_JohnnySargento_001_Winner != None
        W05_MQR_203P_JohnnySargento_001_Winner.Start()
    EndIf
EndFunction

Function Fragment_Stage_1800_Item_00()
    SetObjectiveDisplayed(1800)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && W05_MQR_203P_WinnersCup_Blackout != None
        W05_MQR_203P_WinnersCup_Blackout.Cast(playerRef, playerRef)
    EndIf
EndFunction

Function Fragment_Stage_1900_Item_00()
    SetObjectiveDisplayed(1900)
EndFunction

Function Fragment_Stage_2000_Item_00()
    SetObjectiveDisplayed(2000)
EndFunction

Function Fragment_Stage_2100_Item_00()
    SetObjectiveDisplayed(2100)
    If W05_MQR_203P_HalJohnny_001_Shoot != None
        W05_MQR_203P_HalJohnny_001_Shoot.Start()
    EndIf
EndFunction

Function Fragment_Stage_2110_Item_00()
    Actor halRef = Alias_Hal.GetActorReference()
    Actor johnnyRef = Alias_JohnnyArena.GetActorReference()
    If halRef != None && !halRef.IsDead()
        halRef.Kill(johnnyRef)
    EndIf
EndFunction

Function Fragment_Stage_2200_Item_00()
    SetObjectiveDisplayed(2200)
EndFunction

Function Fragment_Stage_2210_Item_00()
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_2220_Item_00()
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_2221_Item_00()
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_2300_Item_00()
    SetObjectiveDisplayed(2300)
EndFunction

Function Fragment_Stage_2400_Item_00()
    SetObjectiveDisplayed(2400)
EndFunction

Function Fragment_Stage_5000_Item_00()
    If !IsStageDone(5100)
        SetStage(5100)
    EndIf
EndFunction

Function Fragment_Stage_5100_Item_00()
    SetObjectiveDisplayed(5100)
EndFunction

Function Fragment_Stage_5200_Item_00()
    If !IsStageDone(910)
        SetStage(910)
    EndIf
EndFunction

Function Fragment_Stage_5300_Item_00()
    If !IsStageDone(910)
        SetStage(910)
    EndIf
EndFunction

Function Fragment_Stage_6000_Item_00()
    If !IsStageDone(910)
        SetStage(910)
    EndIf
EndFunction

Function Fragment_Stage_7000_Item_00()
    If !IsStageDone(7100)
        SetStage(7100)
    EndIf
EndFunction

Function Fragment_Stage_7100_Item_00()
    SetObjectiveDisplayed(7100)
EndFunction

Function Fragment_Stage_7200_Item_00()
    If !IsStageDone(1310)
        SetStage(1310)
    EndIf
EndFunction

Function Fragment_Stage_7300_Item_00()
    If !IsStageDone(1310)
        SetStage(1310)
    EndIf
EndFunction

Function Fragment_Stage_8100_Item_00()
    SetObjectiveDisplayed(8100)
EndFunction

Function Fragment_Stage_8200_Item_00()
    SetObjectiveDisplayed(8200)
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Game.GetPlayer()
    If W05_MQR_Choice_QuestStartKeyword != None
        W05_MQR_Choice_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction
