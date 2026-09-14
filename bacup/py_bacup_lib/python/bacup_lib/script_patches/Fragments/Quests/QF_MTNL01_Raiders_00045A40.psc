Actor Function MTNL01_GetPlayer()
    Actor playerRef = Alias_CurrentPlayer.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    Return playerRef
EndFunction

Function MTNL01_SayRoseTopic(Topic akTopic)
    If akTopic == None
        Return
    EndIf

    Actor roseRef = Alias_Rose.GetActorReference()
    If roseRef == None
        roseRef = RDR_Contact_Rose
    EndIf
    If roseRef != None
        roseRef.SayCustom(akTopic)
    EndIf
EndFunction

Function MTNL01_MoveAliasItem(ReferenceAlias akItemAlias, ObjectReference akDestination)
    If akItemAlias == None
        Return
    EndIf

    ObjectReference itemRef = akItemAlias.GetReference()
    ObjectReference destinationRef = akDestination
    If destinationRef == None
        destinationRef = MTNL01_GetPlayer()
    EndIf
    If itemRef != None && destinationRef != None && destinationRef.GetItemCount(itemRef) == 0
        destinationRef.AddItem(itemRef, 1, True)
    EndIf
EndFunction

Function MTNL01_RemoveAliasItem(ReferenceAlias akItemAlias)
    Actor playerRef = MTNL01_GetPlayer()
    If playerRef == None || akItemAlias == None
        Return
    EndIf

    ObjectReference itemRef = akItemAlias.GetReference()
    If itemRef != None
        Int itemCount = playerRef.GetItemCount(itemRef)
        If itemCount > 0
            playerRef.RemoveItem(itemRef, itemCount, True)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    Actor playerRef = MTNL01_GetPlayer()
    If playerRef != None
        PlayerSpeakToRose.ForceRefTo(playerRef)
    EndIf
    SetObjectiveDisplayed(100, True)
    If MTNL01_Raiders_Rose_Intro != None && !MTNL01_Raiders_Rose_Intro.IsPlaying()
        MTNL01_Raiders_Rose_Intro.Start()
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    Actor playerRef = MTNL01_GetPlayer()
    If playerRef != None
        PlayerSpeakToRose.ForceRefTo(playerRef)
    EndIf
    SetObjectiveDisplayed(100, True)
    If MTNL01_Raiders_Rose_Intro != None && !MTNL01_Raiders_Rose_Intro.IsPlaying()
        MTNL01_Raiders_Rose_Intro.Start()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(200, True)
EndFunction

Function Fragment_Stage_0205_Item_00()
    MTNL01_SayRoseTopic(MTNL01_Raiders_EBSTopic_LeavingRose)
EndFunction

Function Fragment_Stage_0210_Item_00()
    SetObjectiveCompleted(200, True)
    SetObjectiveDisplayed(210, True)
    MTNL01_SayRoseTopic(MTNL01_Raiders_EBSTopic_BlackwaterMine)
EndFunction

Function Fragment_Stage_0211_Item_00()
    DefaultQuestEncounterWaveScript waveController = (Self as Quest) as DefaultQuestEncounterWaveScript
    If waveController != None
        waveController.StartLocalEncounterWave(0)
    EndIf
EndFunction

Function Fragment_Stage_0220_Item_00()
    MTNL01_MoveAliasItem(Alias_BlackwaterBanditsKey, Alias_BlackwaterMineGlowingOne.GetAt(0))
EndFunction

Function Fragment_Stage_0250_Item_00()
    SetObjectiveCompleted(210, True)
    SetObjectiveDisplayed(250, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(250, True)
    SetObjectiveDisplayed(300, True)
    ObjectReference dispenserRef = Alias_TrappersNoteDispenser.GetReference()
    If dispenserRef != None
        dispenserRef.EnableNoWait()
    EndIf
    If !IsStageDone(303)
        SetStage(303)
    EndIf
    MTNL01_SayRoseTopic(MTNL01_Raiders_EBSTopic_Blackwater02)
EndFunction

Function Fragment_Stage_0310_Item_00()
    SetObjectiveCompleted(300, True)
    SetObjectiveDisplayed(310, True)
    MTNL01_MoveAliasItem(Alias_TrappersKey, Alias_TrappersKeyContainer.GetReference())
    MTNL01_SayRoseTopic(MTNL01_Raiders_EBSTopic_Huntersville)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(310, True)
    SetObjectiveDisplayed(400, True)
    ObjectReference dispenserRef = Alias_MargieHolotapeDispenser.GetReference()
    If dispenserRef != None
        dispenserRef.EnableNoWait()
    EndIf
    If !IsStageDone(403)
        SetStage(403)
    EndIf
EndFunction

Function Fragment_Stage_0405_Item_00()
    SetObjectiveCompleted(400, True)
    SetObjectiveDisplayed(405, True)
EndFunction

Function Fragment_Stage_0406_Item_00()
    SetObjectiveCompleted(405, True)
    SetObjectiveDisplayed(406, True)
EndFunction

Function Fragment_Stage_0408_Item_00()
    SetObjectiveCompleted(406, True)
    SetObjectiveDisplayed(408, True)
    MTNL01_MoveAliasItem(Alias_AdminPassword, Alias_PalaceAdminPasswordContainer.GetReference())
    MTNL01_SayRoseTopic(MTNL01_Raiders_EBSTopic_PalaceOfTheWindingPath)
EndFunction

Function Fragment_Stage_0410_Item_00()
    SetObjectiveCompleted(408, True)
    SetObjectiveDisplayed(410, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(410, True)
    SetObjectiveDisplayed(500, True)
    MTNL01_MoveAliasItem(Alias_DiehardsKey, MTNL01_GetPlayer())
    ObjectReference dispenserRef = Alias_GourmandsNoteDispenser.GetReference()
    If dispenserRef != None
        dispenserRef.EnableNoWait()
    EndIf
    If !IsStageDone(503)
        SetStage(503)
    EndIf
EndFunction

Function Fragment_Stage_0510_Item_00()
    SetObjectiveCompleted(500, True)
    SetObjectiveDisplayed(510, True)
    MTNL01_SayRoseTopic(MTNL01_Raiders_EBSTopic_WendigoCave)
EndFunction

Function Fragment_Stage_0511_Item_00()
    DefaultQuestEncounterWaveScript waveController = (Self as Quest) as DefaultQuestEncounterWaveScript
    If waveController != None
        waveController.StartLocalEncounterWave(1)
    EndIf
EndFunction

Function Fragment_Stage_0520_Item_00()
    MTNL01_MoveAliasItem(Alias_GourmandsKey, Alias_ProgenitorWendigo.GetAt(0))
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(510, True)
    SetObjectiveDisplayed(600, True)
    If !IsStageDone(603)
        SetStage(603)
    EndIf
EndFunction

Function Fragment_Stage_0610_Item_00()
    MTNL01_MoveAliasItem(Alias_CutthroatsKey, Alias_DavidScorched.GetAt(0))
EndFunction

Function Fragment_Stage_0611_Item_00()
    DefaultQuestEncounterWaveScript waveController = (Self as Quest) as DefaultQuestEncounterWaveScript
    If waveController != None
        waveController.StartLocalEncounterWave(2)
        waveController.StartLocalEncounterWave(3)
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(600, True)
    SetObjectiveDisplayed(700, True)
    If !IsStageDone(703)
        SetStage(703)
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(700, True)
    SetObjectiveDisplayed(800, True)
    MTNL01_RemoveAliasItem(Alias_BlackwaterBanditsKey)
    MTNL01_RemoveAliasItem(Alias_TrappersKey)
    MTNL01_RemoveAliasItem(Alias_DiehardsKey)
    MTNL01_RemoveAliasItem(Alias_GourmandsKey)
    MTNL01_RemoveAliasItem(Alias_CutthroatsKey)
    If !IsStageDone(803)
        SetStage(803)
    EndIf
EndFunction

Function Fragment_Stage_0801_Item_00()
    MTNL01_SayRoseTopic(MTNL01_Raiders_EBSTopic_LeavingRoseAgain)
EndFunction

Function Fragment_Stage_0805_Item_00()
    MTNL01_SayRoseTopic(MTNL01_Raiders_EBSTopic_EnterCharleston)
EndFunction

Function Fragment_Stage_0810_Item_00()
    SetObjectiveCompleted(800, True)
    SetObjectiveDisplayed(810, True)
    MTNL01_SayRoseTopic(MTNL01_Raiders_EBSTopic_Charleston)
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(810, True)
    SetObjectiveDisplayed(900, True)
    If !IsStageDone(903)
        SetStage(903)
    EndIf
    MTNL01_SayRoseTopic(MTNL01_Raiders_EBSTopic_End)
    If !IsStageDone(950)
        SetStage(950)
    EndIf
EndFunction

Function Fragment_Stage_0950_Item_00()
    Actor playerRef = MTNL01_GetPlayer()
    If playerRef == None || MTN_MQ_Missing == None || MTN_MQ_Missing_Quest_Keyword == None || MTN_MQ_Missing_QuestActive_Keyword == None
        Return
    EndIf

    Bool missingQuestTerminal = MTN_MQ_Missing.IsCompleted() || MTN_MQ_Missing.IsStageDone(600)
    If missingQuestTerminal
        Return
    EndIf

    Bool missingQuestRunning = MTN_MQ_Missing.IsRunning()
    Bool missingQuestAccepted = missingQuestRunning || playerRef.HasKeyword(MTN_MQ_Missing_QuestActive_Keyword)
    If !missingQuestRunning && !missingQuestAccepted
        MTN_MQ_Missing_Quest_Keyword.SendStoryEventAndWait(None, playerRef, playerRef)
    EndIf

    missingQuestRunning = MTN_MQ_Missing.IsRunning()
    missingQuestTerminal = MTN_MQ_Missing.IsCompleted() || MTN_MQ_Missing.IsStageDone(600)
    If !missingQuestTerminal && missingQuestRunning && !MTN_MQ_Missing.IsStageDone(300)
        MTN_MQ_Missing.SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(900, True)
    SetObjectiveDisplayed(1000, True)
    ObjectReference cacheRef = Alias_CacheKeyContainer.GetReference()
    If cacheRef != None && MTNL01_RaiderCacheKeycard != None && cacheRef.GetItemCount(MTNL01_RaiderCacheKeycard) == 0
        cacheRef.AddItem(MTNL01_RaiderCacheKeycard, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_1050_Item_00()
    Actor playerRef = MTNL01_GetPlayer()
    If playerRef != None && MTNL01_KeycardReaderValue != None
        playerRef.SetValue(MTNL01_KeycardReaderValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1075_Item_00()
    Actor playerRef = MTNL01_GetPlayer()
    If playerRef != None && MTNL01_KeycardReaderValue != None
        playerRef.SetValue(MTNL01_KeycardReaderValue, 2.0)
    EndIf
    If Alias_LaserGrids != None
        Alias_LaserGrids.DisableAll()
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveCompleted(1000, True)
    SetObjectiveDisplayed(1100, True)
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveCompleted(1100, True)
    MTNL01QuestScript questController = (Self as Quest) as MTNL01QuestScript
    If questController != None
        questController.GiveSinglePlayerFinalReward()
    EndIf
    CompleteQuest()
    If !IsStageDone(1300)
        SetStage(1300)
    EndIf
    Stop()
EndFunction
