Function Fragment_Stage_0400_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        If EN02_JoinedEnclaveValue != None
            playerRef.SetValue(EN02_JoinedEnclaveValue, 1.0)
        EndIf
        If EnclaveFaction != None && !playerRef.IsInFaction(EnclaveFaction)
            playerRef.AddToFaction(EnclaveFaction)
        EndIf
    EndIf
    SetObjectiveCompleted(360, True)
    CompleteQuest()
EndFunction

Function Fragment_Stage_0005_Item_00()
    If Alias_currentPlayer.GetRef() == None
        Alias_currentPlayer.ForceRefTo(Game.GetPlayer())
    EndIf
    SetObjectiveDisplayed(5, True, True)
EndFunction

Function Fragment_Stage_0001_Item_00()
    EN02_MQ_Us_Intro.Start()
EndFunction

Function Fragment_Stage_0007_Item_00()
    EN02_MQ_Us_0007_IDCardUsed.Start()
EndFunction

Function Fragment_Stage_0035_Item_00()
    EN02_MQ_Us_0035_DeconScene.Start()
EndFunction

Function Fragment_Stage_0040_Item_00()
    SetObjectiveCompleted(30, True)
    SetObjectiveDisplayed(42, True, True)
    EN02_MQ_Us_0040_CollectUniform.Start()
EndFunction

Function Fragment_Stage_0045_Item_00()
    EN02_MQ_Us_0045_CollectUniform.Start()
EndFunction

Function Fragment_Stage_0047_Item_00()
    Alias_PlayerCanCollectUniform.ForceRefTo(Game.GetPlayer())
    SetObjectiveCompleted(42, True)
    SetObjectiveDisplayed(47, True, True)
EndFunction

Function Fragment_Stage_0050_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        If playerRef.GetValue(EN02_PlayerCollectedUniformValue) <= 0.0
            playerRef.AddItem(LL_Armor_EnclaveUnderarmor, 1, False)
        EndIf
        playerRef.SetValue(EN02_PlayerCollectedUniformValue, 1.0)
        playerRef.SetValue(EN02_PlayerReceivedUniform, 1.0)
    EndIf
    SetObjectiveCompleted(47, True)
    SetObjectiveDisplayed(60, True, True)
    EN02_MQ_Us_0050_ComeToMODUS.Start()
EndFunction

Function Fragment_Stage_0070_Item_00()
    SetObjectiveCompleted(60, True)
    SetObjectiveDisplayed(90, True, True)
    EN02_MQ_Us_0070_MeetingMODUS.Start()
EndFunction

Function Fragment_Stage_0110_Item_00()
    SetObjectiveCompleted(90, True)
    SetObjectiveDisplayed(115, True, False)
    SetObjectiveDisplayed(130, True, True)
    EN02_MQ_Us_0110_LoungeScene.Start()
EndFunction

Function Fragment_Stage_0120_Item_00()
    SetObjectiveCompleted(130, True)
    SetObjectiveDisplayed(140, True, True)
    SetObjectiveDisplayed(145, True, False)
    EN02_MQ_Us_0120_HandscanComplete.Start()
EndFunction

Function Fragment_Stage_0140_Item_00()
    EN02_MQ_Us_0140_ExamScene.Start()
EndFunction

Function Fragment_Stage_0160_Item_00()
    EN02_MQ_Us_0160_ExamComplete.Start()
EndFunction

Function Fragment_Stage_0170_Item_00()
    SetObjectiveCompleted(140, True)
    SetObjectiveCompleted(145, True)
    SetObjectiveDisplayed(170, True, True)
EndFunction

Function Fragment_Stage_0190_Item_00()
    SetObjectiveCompleted(170, True)
    SetObjectiveDisplayed(190, True, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(185, True)
    SetObjectiveCompleted(190, True)
    SetObjectiveDisplayed(210, True, True)
    EN02_MQ_Us_200_ToSugarGrove.Start()
EndFunction

Function Fragment_Stage_0240_Item_00()
    SetObjectiveCompleted(210, True)
    SetObjectiveDisplayed(240, True, True)
EndFunction

Function Fragment_Stage_0260_Item_00()
    SetObjectiveCompleted(240, True)
    SetObjectiveDisplayed(260, True, True)
    EN02_MQ_Us_0260_ReturnInstructions.Start()
EndFunction

Function Fragment_Stage_0270_Item_00()
    SetObjectiveCompleted(260, True)
    SetObjectiveDisplayed(270, True, True)
    EN02_MQ_Us_0270_DepositInstructions.Start()
EndFunction

Function Fragment_Stage_0280_Item_00()
    SetObjectiveCompleted(270, True)
    SetObjectiveDisplayed(290, True, True)
    EN02_MQ_Us_0280_InstructionProvided.Start()
EndFunction

Function Fragment_Stage_0290_Item_00()
    Alias_PlayerCanCollectModule.ForceRefTo(Game.GetPlayer())
    SetObjectiveDisplayed(290, True, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(290, True)
    SetObjectiveDisplayed(310, True, True)
    EN02_MQ_Us_0300_UsingOverride.Start()
EndFunction

Function Fragment_Stage_0310_Item_00()
    EN02_MQ_Us_0310_OverrideComplete.Start()
EndFunction

Function Fragment_Stage_0330_Item_00()
    SetObjectiveCompleted(310, True)
    EN02_MQ_Us_0330_ConnectionEstablished.Start()
EndFunction

Function Fragment_Stage_0015_Item_00()
    SetObjectiveCompleted(5, True)
    SetObjectiveDisplayed(20, True, True)
EndFunction

Function Fragment_Stage_0030_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveDisplayed(30, True, True)
EndFunction

Function Fragment_Stage_0090_Item_00()
    SetObjectiveCompleted(60, True)
    SetObjectiveDisplayed(90, True, True)
EndFunction

Function Fragment_Stage_0315_Item_00()
    Alias_PlayerCanActivateRadarArray.ForceRefTo(Game.GetPlayer())
EndFunction

Function Fragment_Stage_0320_Item_00()
    SetObjectiveCompleted(310, True)
EndFunction

Function Fragment_Stage_0340_Item_00()
    SetObjectiveDisplayed(350, True, True)
EndFunction

Function Fragment_Stage_0350_Item_00()
    SetObjectiveDisplayed(350, True, True)
EndFunction

Function Fragment_Stage_0360_Item_00()
    SetObjectiveCompleted(350, True)
    SetObjectiveDisplayed(360, True, True)
EndFunction

Function Fragment_Stage_0405_Item_00()
    If Alias_currentPlayer.GetRef() == None
        Alias_currentPlayer.ForceRefTo(Game.GetPlayer())
    EndIf
EndFunction

Function Fragment_Stage_0407_Item_00()
    Stop()
EndFunction

Function Fragment_Stage_0410_Item_00()
    Stop()
EndFunction

Function Fragment_Stage_0998_Item_00()
    Stop()
EndFunction
