MTNM01QuestScript Function MTNM01_GetController()
    Return (Self as Quest) as MTNM01QuestScript
EndFunction

Actor Function MTNM01_GetPlayer()
    ReferenceAlias playerAlias = Alias_currentPlayer
    If playerAlias
        Actor playerRef = playerAlias.GetActorReference()
        If playerRef
            Return playerRef
        EndIf
    EndIf
    Return Game.GetPlayer()
EndFunction

Function MTNM01_SetCheckpoint(Float afCheckpoint)
    Actor playerRef = MTNM01_GetPlayer()
    If playerRef && MTNM01_CheckpointValue && playerRef.GetValue(MTNM01_CheckpointValue) < afCheckpoint
        playerRef.SetValue(MTNM01_CheckpointValue, afCheckpoint)
    EndIf
EndFunction

Function MTNM01_SayRoseTopic(Topic akTopic)
    If !akTopic
        Return
    EndIf

    MTNM01QuestScript controller = MTNM01_GetController()
    Actor roseRef
    If controller && controller.Rose
        roseRef = controller.Rose.GetActorReference()
    EndIf
    If roseRef
        roseRef.Say(akTopic, None, True, MTNM01_GetPlayer())
    EndIf
EndFunction

Function MTNM01_BeginKarmaKill(Int aiRewardValue, Topic akResponse)
    MTNM01QuestScript controller = MTNM01_GetController()
    If controller
        controller.iChemDartRewardValue = aiRewardValue
    EndIf
    MTNM01_SayRoseTopic(akResponse)
    If !IsStageDone(350)
        SetStage(350)
    EndIf
EndFunction

Bool Function MTNM01_PlayerHasBaitMaterials()
    Actor playerRef = MTNM01_GetPlayer()
    Return playerRef && playerRef.GetItemCount(fragMine) >= 1 && playerRef.GetItemCount(RadstagMeat) >= 1 && playerRef.GetItemCount(c_Copper_scrap) >= 1 && playerRef.GetItemCount(c_Adhesive_scrap) >= 2
EndFunction

Function MTNM01_MoveAliasItemToContainer(ReferenceAlias akItemAlias, ObjectReference akContainer)
    If !akItemAlias || !akContainer
        Return
    EndIf

    ObjectReference itemRef = akItemAlias.GetReference()
    If itemRef
        Form baseItem = itemRef.GetBaseObject()
        Actor playerRef = MTNM01_GetPlayer()
        If baseItem && playerRef.GetItemCount(baseItem) < 1 && akContainer.GetItemCount(baseItem) < 1
            akContainer.AddItem(itemRef, 1, True)
        EndIf
    EndIf
EndFunction

Function MTNM01_SeedBaitMaterials()
    ObjectReference materialContainer = Alias_BaitMaterialsContainer.GetReference()
    If !materialContainer
        Return
    EndIf

    MTNM01_MoveAliasItemToContainer(Alias_Mine, materialContainer)
    MTNM01_MoveAliasItemToContainer(Alias_RadstagMeatBait, materialContainer)
    MTNM01_MoveAliasItemToContainer(Alias_Copper, materialContainer)
    MTNM01_MoveAliasItemToContainer(Alias_Adhesive01, materialContainer)

    ObjectReference secondAdhesive = Alias_Adhesive02.GetReference()
    Actor playerRef = MTNM01_GetPlayer()
    If secondAdhesive && playerRef.GetItemCount(c_Adhesive_scrap) + materialContainer.GetItemCount(c_Adhesive_scrap) < 2
        materialContainer.AddItem(secondAdhesive, 1, True)
    EndIf
EndFunction

Function MTNM01_CheckBaitMaterials()
    If MTNM01_PlayerHasBaitMaterials()
        SetObjectiveCompleted(420)
        SetObjectiveDisplayed(400, True)
        Actor playerRef = MTNM01_GetPlayer()
        If playerRef && MTNM01_BaitMaterialsGiven_Value
            playerRef.SetValue(MTNM01_BaitMaterialsGiven_Value, 1.0)
        EndIf
    Else
        SetObjectiveDisplayed(420, True)
    EndIf
EndFunction

Function MTNM01_ClearBaitAlias(ReferenceAlias akItemAlias)
    If akItemAlias
        akItemAlias.Clear()
    EndIf
    MTNM01_CheckBaitMaterials()
EndFunction

Function Fragment_Stage_0001_Item_00()
    Actor playerRef = MTNM01_GetPlayer()
    If !playerRef
        Return
    EndIf

    If playerRef.GetItemCount(Psycho) < 1
        playerRef.AddItem(Psycho, 1, True)
    EndIf
    If playerRef.GetItemCount(FirecrackerBerryFruit) < 1
        playerRef.AddItem(FirecrackerBerryFruit, 1, True)
    EndIf
    If playerRef.GetItemCount(SapVegetable) < 1
        playerRef.AddItem(SapVegetable, 1, True)
    EndIf
    If playerRef.GetItemCount(RadstagMeat) < 1
        playerRef.AddItem(RadstagMeat, 1, True)
    EndIf
    If playerRef.GetItemCount(fragMine) < 1
        playerRef.AddItem(fragMine, 1, True)
    EndIf
    If playerRef.GetItemCount(c_Steel_scrap) < 1
        playerRef.AddItem(c_Steel_scrap, 1, True)
    EndIf
    If playerRef.GetItemCount(c_Copper_scrap) < 1
        playerRef.AddItem(c_Copper_scrap, 1, True)
    EndIf
    If playerRef.GetItemCount(c_Adhesive_scrap) < 2
        playerRef.AddItem(c_Adhesive_scrap, 2 - playerRef.GetItemCount(c_Adhesive_scrap), True)
    EndIf
    If playerRef.GetItemCount(AmmoSyringer) < 20
        playerRef.AddItem(AmmoSyringer, 20 - playerRef.GetItemCount(AmmoSyringer), True)
    EndIf
    If !playerRef.HasPerk(Cannibal01)
        playerRef.AddPerk(Cannibal01)
    EndIf
    If !playerRef.HasPerk(MTNM01_DeathclawFriend)
        playerRef.AddPerk(MTNM01_DeathclawFriend)
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    Actor playerRef = MTNM01_GetPlayer()
    Float checkpoint
    If playerRef && MTNM01_CheckpointValue
        checkpoint = playerRef.GetValue(MTNM01_CheckpointValue)
    EndIf

    If checkpoint >= 9.0
        SetStage(901)
    ElseIf checkpoint >= 8.0
        SetStage(900)
    ElseIf checkpoint >= 7.0
        SetStage(800)
    ElseIf checkpoint >= 6.0
        SetStage(700)
    ElseIf checkpoint >= 5.0
        SetStage(600)
    ElseIf checkpoint >= 4.0
        SetStage(500)
    ElseIf checkpoint >= 3.0
        SetStage(400)
    ElseIf checkpoint >= 2.0
        SetStage(300)
    ElseIf checkpoint >= 1.0
        SetStage(200)
    Else
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100, True)
    If MTNM01_Mayhem_Rose_IntroScene
        MTNM01_Mayhem_Rose_IntroScene.Start()
    Else
        MTNM01_SayRoseTopic(MTNM01_Mayhem_EBSTopic_Start)
    EndIf
EndFunction

Function Fragment_Stage_0125_Item_00()
    Actor playerRef = MTNM01_GetPlayer()
    ObjectReference syringerRef = Alias_RoseSyringer.GetReference()
    If playerRef && syringerRef && playerRef.GetItemCount(syringerRef.GetBaseObject()) < 1
        playerRef.AddItem(syringerRef, 1, True)
    EndIf
    If playerRef && playerRef.GetItemCount(AmmoSyringer) < 10
        playerRef.AddItem(AmmoSyringer, 10 - playerRef.GetItemCount(AmmoSyringer), True)
    EndIf
    SetObjectiveCompleted(100)
    MTNM01_SetCheckpoint(1.0)
    If !IsStageDone(200)
        SetStage(200)
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    Actor playerRef = MTNM01_GetPlayer()
    If playerRef && MTNM01_HeardRoseBroadcast01Value
        playerRef.SetValue(MTNM01_HeardRoseBroadcast01Value, 1.0)
    EndIf
    MTNM01_SayRoseTopic(MTNM01_Mayhem_EBSTopic_Returning)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveDisplayed(200, True)
    MTNM01_SayRoseTopic(MTNM01_Mayhem_EBSTopic_Chemdart)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(200)
    SetObjectiveDisplayed(300, True)
    MTNM01_SetCheckpoint(2.0)
    MTNM01_SayRoseTopic(MTNM01_Mayhem_EBSTopic_Chemdart)
EndFunction

Function Fragment_Stage_0310_Item_00()
    MTNM01_BeginKarmaKill(0, MTNM01_Mayhem_EBSTopic_ChemdartRobot)
EndFunction

Function Fragment_Stage_0311_Item_00()
    MTNM01_BeginKarmaKill(1, MTNM01_Mayhem_EBSTopic_ChemdartEasy)
EndFunction

Function Fragment_Stage_0312_Item_00()
    MTNM01_BeginKarmaKill(2, MTNM01_Mayhem_EBSTopic_ChemdartOther)
EndFunction

Function Fragment_Stage_0313_Item_00()
    MTNM01_BeginKarmaKill(3, MTNM01_Mayhem_EBSTopic_ChemdartYaoGuai)
EndFunction

Function Fragment_Stage_0314_Item_00()
    MTNM01_BeginKarmaKill(4, MTNM01_Mayhem_EBSTopic_ChemdartDifficult)
EndFunction

Function Fragment_Stage_0315_Item_00()
    MTNM01_BeginKarmaKill(5, MTNM01_Mayhem_EBSTopic_ChemdartPlayer)
EndFunction

Function Fragment_Stage_0350_Item_00()
    SetObjectiveCompleted(300)
    SetObjectiveDisplayed(350, True)
    MTNM01_SayRoseTopic(MTNM01_Mayhem_EBSTopic_Watching)
EndFunction

Function Fragment_Stage_0351_Item_00()
    SetObjectiveCompleted(350)
    MTNM01QuestScript controller = MTNM01_GetController()
    If controller && controller.KarmaCreature
        controller.KarmaCreature.Clear()
    EndIf
    If !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0352_Item_00()
    SetObjectiveFailed(350)
    MTNM01QuestScript controller = MTNM01_GetController()
    If controller && controller.KarmaCreature
        controller.KarmaCreature.Clear()
    EndIf
    If !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    MTNM01_SetCheckpoint(3.0)
    SetObjectiveDisplayed(400, True)
    MTNM01_SeedBaitMaterials()
    MTNM01_CheckBaitMaterials()
    MTNM01_SayRoseTopic(MTNM01_Mayhem_EBSTopic_MakeBait)
EndFunction

Function Fragment_Stage_0420_Item_00()
    SetObjectiveDisplayed(420, True)
    MTNM01_CheckBaitMaterials()
EndFunction

Function Fragment_Stage_0421_Item_00()
    MTNM01_ClearBaitAlias(Alias_Mine)
EndFunction

Function Fragment_Stage_0422_Item_00()
    MTNM01_ClearBaitAlias(Alias_RadstagMeatBait)
EndFunction

Function Fragment_Stage_0423_Item_00()
    MTNM01_ClearBaitAlias(Alias_Copper)
EndFunction

Function Fragment_Stage_0424_Item_00()
    MTNM01_ClearBaitAlias(Alias_Adhesive01)
EndFunction

Function Fragment_Stage_0425_Item_00()
    MTNM01_ClearBaitAlias(Alias_Adhesive02)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(400)
    If IsObjectiveDisplayed(420)
        SetObjectiveCompleted(420)
    EndIf
    SetObjectiveDisplayed(500, True)
    MTNM01_SetCheckpoint(4.0)
    MTNM01_SayRoseTopic(MTNM01_Mayhem_EBSTopic_UseBait)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(500)
    SetObjectiveDisplayed(600, True)
    MTNM01_SetCheckpoint(5.0)
    Actor playerRef = MTNM01_GetPlayer()
    If playerRef && !playerRef.HasPerk(MTNM01_DeathclawFriend)
        playerRef.AddPerk(MTNM01_DeathclawFriend)
    EndIf
    MTNM01_SayRoseTopic(MTNM01_Mayhem_EBSTopic_Deathclaw01)
EndFunction

Function Fragment_Stage_0650_Item_00()
    SetObjectiveCompleted(600)
    SetObjectiveDisplayed(650, True)
    SetObjectiveDisplayed(651, True)
    MTNM01_SayRoseTopic(MTNM01_Mayhem_EBSTopic_Deathclaw02)
EndFunction

Function Fragment_Stage_0650_Item_01()
    MTNM01QuestScript controller = MTNM01_GetController()
    Actor deathclawRef
    If controller && controller.Deathclaw
        deathclawRef = controller.Deathclaw.GetActorReference()
    EndIf
    Actor playerRef = MTNM01_GetPlayer()
    If deathclawRef && playerRef && !deathclawRef.IsDead()
        deathclawRef.StartCombat(playerRef)
    EndIf
    If playerRef && playerRef.HasPerk(MTNM01_DeathclawFriend)
        playerRef.RemovePerk(MTNM01_DeathclawFriend)
    EndIf
EndFunction

Function Fragment_Stage_0660_Item_00()
    SetObjectiveCompleted(650)
    SetObjectiveFailed(651)
    MTNM01_SayRoseTopic(MTNM01_Mayhem_EBSTopic_KilledDeathclawNormal)
    If !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0661_Item_00()
    SetObjectiveCompleted(651)
    SetObjectiveFailed(650)
    MTNM01_SayRoseTopic(MTNM01_Mayhem_EBSTopic_FledDeathclaw)
    If !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveDisplayed(700, True)
    MTNM01_SetCheckpoint(6.0)
    MTNM01QuestScript controller = MTNM01_GetController()
    If controller && controller.Deathclaw
        controller.Deathclaw.Clear()
    EndIf
    MTNM01_SayRoseTopic(MTNM01_Mayhem_EBSTopic_Bandits)
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(700)
    If IsObjectiveDisplayed(750)
        SetObjectiveCompleted(750)
    EndIf
    SetObjectiveDisplayed(800, True)
    MTNM01_SetCheckpoint(7.0)

    Actor playerRef = MTNM01_GetPlayer()
    ObjectReference rewardRef = Alias_SuperMutantReward.GetReference()
    If playerRef && rewardRef && playerRef.GetItemCount(rewardRef.GetBaseObject()) < 1
        playerRef.AddItem(rewardRef, 1, True)
    EndIf
    MTNM01_SayRoseTopic(MTNM01_Mayhem_EBSTopic_Cannibal)
EndFunction

Function Fragment_Stage_0800_Item_01()
    DefaultQuestOnKillManager killManager = (Self as Quest) as DefaultQuestOnKillManager
    If killManager
        killManager.StartTrackingKills()
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(800)
    SetObjectiveDisplayed(900, True)
    SetObjectiveDisplayed(901, True)
    MTNM01_SetCheckpoint(8.0)

    MTNM01QuestScript controller = MTNM01_GetController()
    If controller
        Actor killedGhoul = controller.PlayerKilledGhoul.GetActorReference()
        If killedGhoul && controller.GhoulCorpse
            controller.GhoulCorpse.ForceRefTo(killedGhoul)
        EndIf
    EndIf

    Actor playerRef = MTNM01_GetPlayer()
    If playerRef && !playerRef.HasPerk(Cannibal01)
        playerRef.AddPerk(Cannibal01)
    EndIf
    MTNM01_SayRoseTopic(MTNM01_Mayhem_EBSTopic_CannibalOptional)
EndFunction

Function Fragment_Stage_0901_Item_00()
    SetObjectiveCompleted(901)
    SetObjectiveDisplayed(900, True)
    MTNM01_SetCheckpoint(9.0)
    If !IsStageDone(903)
        SetStage(903)
    EndIf
EndFunction

Function Fragment_Stage_0960_Item_00()
    If IsObjectiveDisplayed(901) && !IsObjectiveCompleted(901)
        SetObjectiveFailed(901)
    EndIf
    MTNM01QuestScript controller = MTNM01_GetController()
    If controller && controller.GhoulCorpse
        controller.GhoulCorpse.Clear()
    EndIf
    MTNM01_SayRoseTopic(MTNM01_Mayhem_EBSTopic_Returning)
EndFunction

Function Fragment_Stage_1000_Item_00()
    Actor playerRef = MTNM01_GetPlayer()
    CompleteAllObjectives()
    MTNM01_SetCheckpoint(10.0)
    If playerRef
        If playerRef.HasPerk(MTNM01_DeathclawFriend)
            playerRef.RemovePerk(MTNM01_DeathclawFriend)
        EndIf
        If playerRef.HasPerk(Cannibal01)
            playerRef.RemovePerk(Cannibal01)
        EndIf
        If MTNL01_Raiders_Quest_Keyword
            MTNL01_Raiders_Quest_Keyword.SendStoryEventAndWait(None, playerRef, playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    Actor playerRef = MTNM01_GetPlayer()
    If playerRef
        If playerRef.HasPerk(MTNM01_DeathclawFriend)
            playerRef.RemovePerk(MTNM01_DeathclawFriend)
        EndIf
        If playerRef.HasPerk(Cannibal01)
            playerRef.RemovePerk(Cannibal01)
        EndIf
    EndIf

    Alias_TRIGGERYaoGuai.Clear()
    Alias_TRIGGERBait.Clear()
    Alias_TRIGGERDeathclaw.Clear()
    Alias_TRIGGERGhoul.Clear()
    Alias_SuperMutantContainer.Clear()
    Alias_SuperMutantReward.Clear()
    Alias_RoseSyringer.Clear()
    Alias_Mine.Clear()
    Alias_RadstagMeatBait.Clear()
    Alias_Copper.Clear()
    Alias_Adhesive01.Clear()
    Alias_Adhesive02.Clear()
    PlayerSpeakToRose.Clear()
EndFunction
