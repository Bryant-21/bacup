Function Fragment_Stage_0010_Item_00()
    If GetStage() < 50
        SetStage(50)
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0051_Item_00()
    SetObjectiveCompleted(50)
EndFunction

Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && !playerRef.HasPerk(W05_MQS_202P_CollectLiberatorPerk)
        playerRef.AddPerk(W05_MQS_202P_CollectLiberatorPerk)
    EndIf
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0075_Item_00()
    If GetStage() < 100
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        If playerRef.HasPerk(W05_MQS_202P_CollectLiberatorPerk)
            playerRef.RemovePerk(W05_MQS_202P_CollectLiberatorPerk)
        EndIf
        If playerRef.GetItemCount(W05_MQS_202_MiscItem_DeactivatedLiberator) < 1
            playerRef.AddItem(W05_MQS_202_MiscItem_DeactivatedLiberator, 1, False)
        EndIf
        playerRef.SetValue(W05_MQS_202P_PlayerFoundLiberator, 1.0)
    EndIf
    SetObjectiveCompleted(100)
    If GetStage() < 200
        SetStage(200)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0201_Item_00()
    SetObjectiveCompleted(200)
    If W05_MQS_202P_Scene1a != None
        W05_MQS_202P_Scene1a.Start()
    EndIf
EndFunction

Function Fragment_Stage_0202_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_202P_CanPlaceLiberator, 1.0)
    EndIf
    ObjectReference enableMarker = Alias_HQ_Instance_Enable_Liberator.GetReference()
    If enableMarker != None
        enableMarker.Enable()
    EndIf
EndFunction

Function Fragment_Stage_0210_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.RemoveItem(W05_MQS_202_MiscItem_DeactivatedLiberator, 1, True)
        If playerRef.GetItemCount(W05_MQS_202P_MiscItem_RecalibratedLiberator) < 1
            playerRef.AddItem(W05_MQS_202P_MiscItem_RecalibratedLiberator, 1, False)
        EndIf
        playerRef.SetValue(W05_MQS_202P_CanPlaceLiberator, 0.0)
    EndIf
    If !IsStageDone(211)
        SetStage(211)
    EndIf
EndFunction

Function Fragment_Stage_0211_Item_00()
    ObjectReference corpseRef = Alias_CollectedLiberator.GetReference()
    If corpseRef != None
        corpseRef.Disable()
    EndIf
    ObjectReference corpseEnableMarker = Alias_HQ_Instance_Enable_Liberator.GetReference()
    If corpseEnableMarker != None
        corpseEnableMarker.Disable()
    EndIf
    If GetStage() < 225
        SetStage(225)
    EndIf
EndFunction

Function Fragment_Stage_0225_Item_00()
    SetObjectiveDisplayed(225)
    If W05_MQS_202P_Scene1b != None
        W05_MQS_202P_Scene1b.Start()
    EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
    SetObjectiveCompleted(225)
    SetObjectiveDisplayed(250)
EndFunction

Function Fragment_Stage_0251_Item_00()
    SetObjectiveCompleted(250)
    If W05_MQS_202P_Scene2 != None && !W05_MQS_202P_Scene2.IsPlaying()
        W05_MQS_202P_Scene2.Start()
    EndIf
EndFunction

Function Fragment_Stage_0275_Item_00()
    SetObjectiveCompleted(250)
    If GetStage() < 300
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && playerRef.GetItemCount(W05_MQS_202P_MiscItem_RecalibratedLiberator) < 1
        playerRef.AddItem(W05_MQS_202P_MiscItem_RecalibratedLiberator, 1, False)
    EndIf
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(300)
    W05_MQS_202P_QuestScript controller = (Self as Quest) as W05_MQS_202P_QuestScript
    If controller != None
        controller.DeployRecalibratedLiberator()
    EndIf
    If W05_MQS_202P_Scene2 != None && !W05_MQS_202P_Scene2.IsPlaying()
        W05_MQS_202P_Scene2.Start()
    EndIf
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(500)
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(600)
    SetObjectiveDisplayed(700)
EndFunction

Function Fragment_Stage_0720_Item_00()
    SetObjectiveCompleted(700)
    SetObjectiveDisplayed(720)
    If W05_MQS_202P_Scene_EnterDeep != None
        W05_MQS_202P_Scene_EnterDeep.Start()
    EndIf
EndFunction

Function Fragment_Stage_0721_Item_00()
    If W05_MQS_202P_Scene_EnterDeep3 != None
        W05_MQS_202P_Scene_EnterDeep3.Start()
    EndIf
EndFunction

Function Fragment_Stage_0725_Item_00()
    SetObjectiveCompleted(720)
    SetObjectiveDisplayed(725)
EndFunction

Function Fragment_Stage_0726_Item_00()
    SetObjectiveCompleted(725)
    If W05_MQS_202P_Scene_EnterDeep4 != None && !W05_MQS_202P_Scene_EnterDeep4.IsPlaying()
        W05_MQS_202P_Scene_EnterDeep4.Start()
    ElseIf W05_MQS_202P_Scene_EnterDeep4 == None && !IsStageDone(727)
        SetStage(727)
    EndIf
EndFunction

Function Fragment_Stage_0727_Item_00()
    SetObjectiveDisplayed(727)
EndFunction

Function Fragment_Stage_0730_Item_00()
    SetObjectiveCompleted(725)
    Actor spyRef = Alias_TL_Instance_Actor_Spy.GetActorReference()
    If spyRef != None && spyRef.GetActorBase() != None
        spyRef.GetActorBase().SetEssential(False)
    EndIf
EndFunction

Function Fragment_Stage_0746_Item_00()
    SetObjectiveCompleted(727)
EndFunction

Function Fragment_Stage_0747_Item_00()
    SetObjectiveDisplayed(747)
EndFunction

Function Fragment_Stage_0748_Item_00()
    SetObjectiveDisplayed(748)
EndFunction

Function Fragment_Stage_0749_Item_00()
    SetObjectiveCompleted(747)
    SetObjectiveCompleted(748)
    If !IsStageDone(750)
        SetStage(750)
    EndIf
EndFunction

Function Fragment_Stage_0750_Item_00()
    If W05_MQS_202P_Scene4_Final_JI != None
        W05_MQS_202P_Scene4_Final_JI.Start()
    EndIf
EndFunction

Function Fragment_Stage_0751_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_202P_SpyIsAlive, 0.0)
    EndIf
EndFunction

Function Fragment_Stage_0752_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_202P_SpyIsAlive, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(747)
    SetObjectiveCompleted(748)
    SetObjectiveDisplayed(800)
EndFunction

Function Fragment_Stage_0851_Item_00()
    SetObjectiveCompleted(800)
    If !IsStageDone(899)
        SetStage(899)
    EndIf
EndFunction

Function Fragment_Stage_0899_Item_00()
    If GetStage() < 900
        SetStage(900)
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveDisplayed(900)
EndFunction

Function Fragment_Stage_0999_Item_00()
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        SetObjectiveCompleted(900)
        If playerRef.HasPerk(W05_MQS_202P_CollectLiberatorPerk)
            playerRef.RemovePerk(W05_MQS_202P_CollectLiberatorPerk)
        EndIf
        playerRef.RemoveItem(W05_MQS_202_MiscItem_DeactivatedLiberator, 1, True)
        playerRef.RemoveItem(W05_MQS_202P_MiscItem_RecalibratedLiberator, 1, True)
        playerRef.SetValue(W05_MQS_202P_CanPlaceLiberator, 0.0)
        playerRef.SetValue(W05_MQS_202P_QuestComplete, 1.0)
        playerRef.SetValue(W05_JenIsInFoundation, 1.0)
        playerRef.SetValue(W05_PennyIsInFoundation, 1.0)
    EndIf
    CompleteAllObjectives()
    ; FO76 reputation and account rewards are server-owned and have no FO4-local mutation.
    If W05_MQS_203P_QuestStartKeyword != None
        W05_MQS_203P_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
    If !IsStageDone(10000)
        SetStage(10000)
    EndIf
EndFunction

Function Fragment_Stage_9999_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        If playerRef.HasPerk(W05_MQS_202P_CollectLiberatorPerk)
            playerRef.RemovePerk(W05_MQS_202P_CollectLiberatorPerk)
        EndIf
        playerRef.RemoveItem(W05_MQS_202_MiscItem_DeactivatedLiberator, 1, True)
        playerRef.RemoveItem(W05_MQS_202P_MiscItem_RecalibratedLiberator, 1, True)
        playerRef.SetValue(W05_MQS_202P_CanPlaceLiberator, 0.0)
    EndIf
    FailAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_10000_Item_00()
    ObjectReference collectedLiberator = Alias_CollectedLiberator.GetReference()
    If collectedLiberator != None
        collectedLiberator.Disable()
    EndIf
    ObjectReference deployedLiberator = Alias_Actor_RecalibratedLiberator.GetReference()
    If deployedLiberator != None
        deployedLiberator.Disable()
    EndIf
    ObjectReference enableMarker = Alias_HQ_Instance_Enable_Liberator.GetReference()
    If enableMarker != None
        enableMarker.Disable()
    EndIf
    Stop()
EndFunction
