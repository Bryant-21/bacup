; TODO

Function Fragment_Stage_0010_Item_00()
    If GetStage() < 100
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveDisplayed(150)
EndFunction

Function Fragment_Stage_0175_Item_00()
    If GetStage() < 200
        SetStage(200)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0225_Item_00()
    If W05_MQS_201P_Scene1 != None
        W05_MQS_201P_Scene1.Start()
    EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
    If GetStage() < 300
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0310_Item_00()
    SetObjectiveDisplayed(310)
EndFunction

Function Fragment_Stage_0325_Item_00()
    SetObjectiveDisplayed(325)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0425_Item_00()
    SetObjectiveDisplayed(425)
EndFunction

Function Fragment_Stage_0426_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_201P_KeycardClue01Found, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0427_Item_00()
    SetObjectiveDisplayed(427)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_201P_KeycardClue02Found, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0428_Item_00()
    SetObjectiveDisplayed(428)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_201P_KeycardClue03Found, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0429_Item_00()
    SetObjectiveDisplayed(429)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_201P_KeycardClue04Found, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0449_Item_00()
    SetObjectiveDisplayed(449)
EndFunction

Function Fragment_Stage_0450_Item_00()
    SetObjectiveDisplayed(450)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && playerRef.GetItemCount(W05_MQS_201P_HornwrightSaferoomKeycard) < 1
        playerRef.AddItem(W05_MQS_201P_HornwrightSaferoomKeycard, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_0490_Item_00()
    If W05_MQS_201P_Scene_Safe_Room != None
        W05_MQS_201P_Scene_Safe_Room.Start()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_0510_Item_00()
    If W05_MQS_201P_Scene2a != None
        W05_MQS_201P_Scene2a.Start()
    EndIf
EndFunction

Function Fragment_Stage_0525_Item_00()
    If W05_MQS_201P_Scene2b != None
        W05_MQS_201P_Scene2b.Start()
    EndIf
EndFunction

Function Fragment_Stage_0550_Item_00()
    If GetStage() < 600
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveDisplayed(700)
EndFunction

Function Fragment_Stage_0625_Item_00()
    If !IsStageDone(700)
        SetStage(700)
    EndIf
    If !IsStageDone(725)
        SetStage(725)
    EndIf
EndFunction

Function Fragment_Stage_0725_Item_00()
    SetObjectiveDisplayed(725)
EndFunction

Function Fragment_Stage_0730_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_201P_PlayerFoundPipboyKit, 1.0)
    EndIf
    If IsStageDone(731) && !IsStageDone(750)
        SetStage(750)
    EndIf
EndFunction

Function Fragment_Stage_0731_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_201P_PlayerFoundPipboyPhoto, 1.0)
    EndIf
    If IsStageDone(730) && !IsStageDone(750)
        SetStage(750)
    EndIf
EndFunction

Function Fragment_Stage_0750_Item_00()
    If GetStage() < 800
        SetStage(800)
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveDisplayed(800)
EndFunction

Function Fragment_Stage_0810_Item_00()
    If W05_MQS_201P_Scene3 != None
        W05_MQS_201P_Scene3.Start()
    EndIf
EndFunction

Function Fragment_Stage_0825_Item_00()
    If GetStage() < 850
        SetStage(850)
    EndIf
EndFunction

Function Fragment_Stage_0850_Item_00()
    SetObjectiveDisplayed(850)
EndFunction

Function Fragment_Stage_0851_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.RemoveItem(W05_MQS_201P_MiscItem_PipboyKit, 1, True)
        playerRef.RemoveItem(W05_MQS_201P_MiscItem_PipboySchematic, 1, True)
    EndIf
    Alias_CollectedPipboyKit.Clear()
    Alias_CollectedPipboySchematic.Clear()
EndFunction

Function Fragment_Stage_0875_Item_00()
    If !IsStageDone(900)
        SetStage(900)
    EndIf
    If !IsStageDone(925)
        SetStage(925)
    EndIf
    If !IsStageDone(950)
        SetStage(950)
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveDisplayed(900)
EndFunction

Function Fragment_Stage_0901_Item_00()
    SetObjectiveDisplayed(901)
EndFunction

Function Fragment_Stage_0902_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_201P_PlayerBypassVertibotPart, 1.0)
    EndIf
    If !IsStageDone(951)
        SetStage(951)
    EndIf
EndFunction

Function Fragment_Stage_0925_Item_00()
    SetObjectiveDisplayed(925)
EndFunction

Function Fragment_Stage_0950_Item_00()
    SetObjectiveDisplayed(950)
EndFunction

Function Fragment_Stage_0951_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_201P_PlayerFoundVertibotPart, 1.0)
    EndIf
    If IsStageDone(952) && IsStageDone(953) && !IsStageDone(975)
        SetStage(975)
    EndIf
EndFunction

Function Fragment_Stage_0952_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_201P_PlayerFoundEyebotPart, 1.0)
    EndIf
    If IsStageDone(951) && IsStageDone(953) && !IsStageDone(975)
        SetStage(975)
    EndIf
EndFunction

Function Fragment_Stage_0953_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_201P_PlayerFoundRobobrainPart, 1.0)
    EndIf
    If IsStageDone(951) && IsStageDone(952) && !IsStageDone(975)
        SetStage(975)
    EndIf
EndFunction

Function Fragment_Stage_0975_Item_00()
    If GetStage() < 1000
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveDisplayed(1000)
EndFunction

Function Fragment_Stage_1010_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.RemoveItem(W05_MQS_201P_MiscItem_EyebotPart, 1, True)
        playerRef.RemoveItem(W05_MQS_201P_MiscItem_RobobrainPart, 1, True)
        playerRef.RemoveItem(W05_MQS_201P_MiscItem_VertibotPart, 1, True)
    EndIf
    Alias_CollectedRobotPartEyebot.Clear()
    Alias_CollectedRobotPartRobobrain.Clear()
    Alias_CollectedRobotPartVertibot.Clear()
EndFunction

Function Fragment_Stage_1025_Item_00()
    If W05_MQS_201P_Scene4 != None
        W05_MQS_201P_Scene4.Start()
        Utility.Wait(0.1)
        While W05_MQS_201P_Scene4.IsPlaying() && !IsStageDone(1200)
            Utility.Wait(0.5)
        EndWhile
        If IsStageDone(1050) && !IsStageDone(1200)
            SetStage(1200)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveDisplayed(1200)
EndFunction

Function Fragment_Stage_1225_Item_00()
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_201P_QuestComplete, 1.0)
        playerRef.SetValue(W05_PennyIsInFoundation, 1.0)
    EndIf
    If W05_MQS_202P_QuestStartKeyword != None
        W05_MQS_202P_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction
