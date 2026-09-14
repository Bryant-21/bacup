Function Fragment_Stage_0000_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef && playerRef.GetItemCount(W05_MQ_101P_A_RepairTerminalKey) == 0
        playerRef.AddItem(W05_MQ_101P_A_RepairTerminalKey, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_0001_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.AddItem(c_Circuitry_scrap, 1, True)
        playerRef.AddItem(c_Copper_scrap, 1, True)
        playerRef.AddItem(c_Glass_scrap, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_0002_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef && playerRef.GetItemCount(W05_MQ_101P_A_DavidTrophy) == 0
        playerRef.AddItem(W05_MQ_101P_A_DavidTrophy, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_0003_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef && playerRef.GetItemCount(W05_MQ_101P_A_MemorialPhoto) == 0
        playerRef.AddItem(W05_MQ_101P_A_MemorialPhoto, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_0004_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef && playerRef.GetItemCount(W05_MQ_101P_A_HookUp) == 0
        playerRef.AddItem(W05_MQ_101P_A_HookUp, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_0005_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef && playerRef.GetItemCount(W05_MQ_101P_A_AIProgramBroken) == 0
        playerRef.AddItem(W05_MQ_101P_A_AIProgramBroken, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_0006_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef && playerRef.GetItemCount(W05_MQ_101P_A_AIProgramFixed) == 0
        playerRef.AddItem(W05_MQ_101P_A_AIProgramFixed, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.SetValue(W05_MQ_101P_A_Started, 1.0)
    EndIf
    If MTNS01_Intro && MTNS01_Intro.IsStageDone(600)
        SetStage(100)
    Else
        SetStage(50)
        If MTNS01_Intro_Quest_Keyword && playerRef
            MTNS01_Intro_Quest_Keyword.SendStoryEvent(None, playerRef, playerRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0100_Item_01()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0310_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.AddItem(Caps001, 10, False)
    EndIf
EndFunction

Function Fragment_Stage_0311_Item_00()
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0320_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef && playerRef.GetItemCount(W05_MQ_101P_A_DavidLetter) == 0
        playerRef.AddItem(W05_MQ_101P_A_DavidLetter, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_0330_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        If playerRef.GetItemCount(W05_MQ_101P_A_MemorialPhoto) == 0
            playerRef.AddItem(W05_MQ_101P_A_MemorialPhoto, 1, False)
        EndIf
        playerRef.AddItem(Stimpak, 1, False)
        If QSTW05MQ101PDeskPanel
            QSTW05MQ101PDeskPanel.Play(playerRef)
        EndIf
    EndIf
    If !IsStageDone(350)
        SetStage(350)
    EndIf
EndFunction

Function Fragment_Stage_0331_Item_00()
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0350_Item_00()
    SetObjectiveDisplayed(350)
EndFunction

Function Fragment_Stage_0375_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef && playerRef.GetItemCount(W05_MQ_101P_A_MemorialPhoto) > 0 && !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveDisplayed(500)
    If !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0650_Item_00()
    SetObjectiveDisplayed(650)
EndFunction

Function Fragment_Stage_0680_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef && playerRef.GetItemCount(W05_MQ_101P_A_RepairTerminalKey) == 0
        playerRef.AddItem(W05_MQ_101P_A_RepairTerminalKey, 1, True)
    EndIf
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveDisplayed(700)
EndFunction

Function Fragment_Stage_0710_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        If playerRef.GetItemCount(W05_MQ_101P_A_AIProgramBroken) > 0
            playerRef.RemoveItem(W05_MQ_101P_A_AIProgramBroken, 1, True)
        EndIf
        If playerRef.GetItemCount(W05_MQ_101P_A_AIProgramFixed) > 0
            playerRef.RemoveItem(W05_MQ_101P_A_AIProgramFixed, 1, True)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0730_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.SetValue(W05_MQ_101P_A_RoseFileAccess, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveDisplayed(800)
EndFunction

Function Fragment_Stage_0810_Item_00()
    If RDR_Contact_Rose && W05_MQ_101P_A_RoseDavid_TopicInfo
        RDR_Contact_Rose.Say(W05_MQ_101P_A_RoseDavid_TopicInfo, None, True)
    EndIf
EndFunction

Function Fragment_Stage_0820_Item_00()
    If !IsStageDone(900)
        SetStage(900)
    EndIf
EndFunction

Function Fragment_Stage_0830_Item_00()
    If RDR_Contact_Rose && W05_MQ_101P_A_RoseFun_TopicInfo
        RDR_Contact_Rose.Say(W05_MQ_101P_A_RoseFun_TopicInfo, None, True)
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveDisplayed(900)
EndFunction

Function Fragment_Stage_0910_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef && playerRef.GetItemCount(W05_MQ_101P_A_DavidHolotapeMeeting) > 0
        playerRef.RemoveItem(W05_MQ_101P_A_DavidHolotapeMeeting, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_0930_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef && playerRef.GetItemCount(W05_MQ_101P_A_HookUp) == 0
        playerRef.AddItem(W05_MQ_101P_A_HookUp, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_0950_Item_00()
    SetObjectiveDisplayed(950)
EndFunction

Function Fragment_Stage_0960_Item_00()
    If W05_MQ_101P_A_1000_DavidScene && !W05_MQ_101P_A_1000_DavidScene.IsPlaying()
        W05_MQ_101P_A_1000_DavidScene.Start()
    EndIf
    If !IsStageDone(970)
        SetStage(970)
    EndIf
EndFunction

Function Fragment_Stage_0970_Item_00()
    SetObjectiveDisplayed(970)
    If RDR_Contact_Rose && W05_MQ_101P_A_RoseEBSTopicInfo
        RDR_Contact_Rose.Say(W05_MQ_101P_A_RoseEBSTopicInfo, None, True)
    EndIf
    If !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveDisplayed(1000)
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.SetValue(W05_MQ_101P_A_TopOfTheWorldValue, 1.0)
    EndIf
    W05_MQ_101P_A_QuestScript controller = (Self as Quest) as W05_MQ_101P_A_QuestScript
    If controller
        controller.PrepareMezzanine()
    EndIf
EndFunction

Function Fragment_Stage_1050_Item_00()
    W05_MQ_101P_A_QuestScript controller = (Self as Quest) as W05_MQ_101P_A_QuestScript
    If controller
        controller.CheckMezzanineHostiles()
    ElseIf !IsStageDone(1100)
        SetStage(1100)
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveDisplayed(1100)
    If !IsStageDone(1110)
        SetStage(1110)
    EndIf
EndFunction

Function Fragment_Stage_1110_Item_00()
    W05_MQ_101P_A_QuestScript controller = (Self as Quest) as W05_MQ_101P_A_QuestScript
    If controller
        controller.SpawnMegParty()
    Else
        Actor megRef = Alias_Meg.GetActorReference()
        If megRef
            megRef.Enable()
            megRef.EvaluatePackage()
        EndIf
    EndIf
    If !IsStageDone(1200)
        SetStage(1200)
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveDisplayed(1200)
EndFunction

Function Fragment_Stage_1210_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef && playerRef.GetItemCount(W05_MQ_101P_A_DavidTrophy) > 0
        playerRef.RemoveItem(W05_MQ_101P_A_DavidTrophy, 1, True)
        playerRef.SetValue(W05_MQ_101P_A_MegHasTrophy, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    SetObjectiveDisplayed(1300)
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.SetValue(W05_RaiderFactionAV, 1.0)
        playerRef.SetValue(Reputation_AllowHostileTier_AV_Crater, 0.0)
    EndIf
    Actor aldridgeRef = Alias_Aldridge.GetActorReference()
    If aldridgeRef
        aldridgeRef.Enable()
        aldridgeRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1400_Item_00()
    SetObjectiveDisplayed(1400)
EndFunction

Function Fragment_Stage_1415_Item_00()
    Actor aldridgeRef = Alias_Aldridge.GetActorReference()
    If aldridgeRef && !aldridgeRef.IsDead()
        aldridgeRef.PlayIdle(ScorchSuicide)
        aldridgeRef.Kill(Game.GetPlayer())
    EndIf
EndFunction

Function Fragment_Stage_1420_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.SetValue(W05_MQ_101P_A_AldridgeDeadValue, 1.0)
        W05_MQ_101P_A_QuestScript deathController = (Self as Quest) as W05_MQ_101P_A_QuestScript
        If deathController
            playerRef.SetValue(deathController.W05_MQ_101P_A_AldridgeCauseOfDeathValue, 0.0)
        EndIf
        playerRef.SetValue(W05_MQ_101P_A_AldridgeScorchedValue, 2.0)
    EndIf
    Actor aldridgeRef = Alias_Aldridge.GetActorReference()
    If aldridgeRef && !aldridgeRef.IsDead()
        aldridgeRef.PlayIdle(ScorchSuicide)
        aldridgeRef.Kill()
    EndIf
    If !IsStageDone(1500)
        SetStage(1500)
    EndIf
EndFunction

Function Fragment_Stage_1430_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        W05_MQ_101P_A_QuestScript deathController = (Self as Quest) as W05_MQ_101P_A_QuestScript
        If deathController
            playerRef.SetValue(deathController.W05_MQ_101P_A_AldridgeCauseOfDeathValue, 2.0)
        EndIf
    EndIf
    Actor aldridgeRef = Alias_Aldridge.GetActorReference()
    If aldridgeRef
        aldridgeRef.RemoveFromFaction(PlayerFriendFaction)
        aldridgeRef.AddToFaction(PlayerEnemyFaction)
        aldridgeRef.StartCombat(Game.GetPlayer())
    EndIf
EndFunction

Function Fragment_Stage_1440_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        W05_MQ_101P_A_QuestScript deathController = (Self as Quest) as W05_MQ_101P_A_QuestScript
        If deathController
            playerRef.SetValue(deathController.W05_MQ_101P_A_AldridgeCauseOfDeathValue, 1.0)
        EndIf
    EndIf
    Actor aldridgeRef = Alias_Aldridge.GetActorReference()
    If aldridgeRef
        aldridgeRef.RemoveFromFaction(PlayerFriendFaction)
        aldridgeRef.AddToFaction(PlayerEnemyFaction)
        aldridgeRef.StartCombat(Game.GetPlayer())
    EndIf
EndFunction

Function Fragment_Stage_1450_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.SetValue(W05_MQ_101P_A_AldridgeDeadValue, 1.0)
        playerRef.SetValue(W05_MQ_101P_A_AldridgeScorchedValue, 2.0)
        If IsStageDone(1410) && !IsStageDone(1420) && !IsStageDone(1430) && !IsStageDone(1440)
            W05_MQ_101P_A_QuestScript deathController = (Self as Quest) as W05_MQ_101P_A_QuestScript
            If deathController
                playerRef.SetValue(deathController.W05_MQ_101P_A_AldridgeCauseOfDeathValue, 3.0)
            EndIf
        EndIf
    EndIf
    If !IsStageDone(1500)
        SetStage(1500)
    EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
    SetObjectiveDisplayed(1500)
EndFunction

Function Fragment_Stage_1530_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.SetValue(W05_PlayerKnows_AppalachiaHasATreasure, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_8000_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.SetValue(W05_MQ_101P_A_AldridgeWatchstationValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.SetValue(W05_RaiderFactionAV, 1.0)
        playerRef.SetValue(Reputation_AllowHostileTier_AV_Crater, 0.0)
    EndIf
    If W05_MQ_101P && !W05_MQ_101P.IsStageDone(200)
        W05_MQ_101P.SetStage(200)
    EndIf
    ; FO76 quest rewards and account reputation are server-owned; native CompleteQuest still runs.
EndFunction
