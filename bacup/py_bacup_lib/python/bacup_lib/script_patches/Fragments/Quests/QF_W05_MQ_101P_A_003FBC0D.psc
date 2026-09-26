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
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0100_Item_01()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(200)
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    ObjectReference trophyMarker = Alias_DavidTrophyMarker.GetReference()
    If Alias_DavidTrophy.GetReference() == None && trophyMarker && (playerRef == None || playerRef.GetItemCount(W05_MQ_101P_A_DavidTrophy) == 0)
        ObjectReference trophyRef = trophyMarker.PlaceAtMe(W05_MQ_101P_A_DavidTrophy, 1, True)
        If trophyRef
            Alias_DavidTrophy.ForceRefTo(trophyRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(200)
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
            ObjectReference photoRef = playerRef.PlaceAtMe(W05_MQ_101P_A_MemorialPhoto, 1, True)
            If photoRef
                Alias_RoseMemorialPhoto.ForceRefTo(photoRef)
                playerRef.AddItem(photoRef, 1, False)
            EndIf
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
    SetObjectiveCompleted(300)
    SetObjectiveDisplayed(350)
EndFunction

Function Fragment_Stage_0375_Item_00()
    Actor roseRef = None
    ReferenceAlias roseAlias = GetAlias(1) as ReferenceAlias
    If roseAlias
        roseRef = roseAlias.GetActorReference()
    EndIf
    If roseRef == None
        roseRef = RDR_Contact_Rose
    EndIf
    If roseRef && W05_MQ_101P_A_RoseDavid_TopicInfo
        roseRef.Say(W05_MQ_101P_A_RoseDavid_TopicInfo, None, True, Alias_currentPlayer.GetReference())
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(350)
    SetObjectiveDisplayed(400)
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    ObjectReference chemBox = Alias_RoseChemBox.GetReference()
    If Alias_RoseAIProgramBroken.GetReference() == None && chemBox && (playerRef == None || playerRef.GetItemCount(W05_MQ_101P_A_AIProgramBroken) == 0)
        ObjectReference programRef = chemBox.PlaceAtMe(W05_MQ_101P_A_AIProgramBroken, 1, True)
        If programRef
            Alias_RoseAIProgramBroken.ForceRefTo(programRef)
            chemBox.AddItem(programRef, 1, True)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(400)
    SetObjectiveDisplayed(500)
    Actor roseRef = None
    ReferenceAlias roseAlias = GetAlias(1) as ReferenceAlias
    If roseAlias
        roseRef = roseAlias.GetActorReference()
    EndIf
    If roseRef == None
        roseRef = RDR_Contact_Rose
    EndIf
    If roseRef && W05_MQ_101P_A_RoseEBSTopicInfo
        roseRef.Say(W05_MQ_101P_A_RoseEBSTopicInfo, None, True, Alias_currentPlayer.GetReference())
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(500)
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0650_Item_00()
    SetObjectiveDisplayed(650)
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    ObjectReference keyAnchor = Alias_RepairTerminalPasswordMarker.GetReference()
    If keyAnchor == None
        keyAnchor = Alias_RepairTerminal.GetReference()
    EndIf
    If Alias_RepairTerminalPassword.GetReference() == None && keyAnchor && (playerRef == None || playerRef.GetItemCount(W05_MQ_101P_A_RepairTerminalKey) == 0)
        ObjectReference keyRef = keyAnchor.PlaceAtMe(W05_MQ_101P_A_RepairTerminalKey, 1, True)
        If keyRef
            Alias_RepairTerminalPassword.ForceRefTo(keyRef)
        EndIf
    EndIf
    If W05_MQ_101P_A_KensingtonCanSpawn
        W05_MQ_101P_A_KensingtonCanSpawn.SetValue(1.0)
    EndIf
EndFunction

Function Fragment_Stage_0680_Item_00()
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef && playerRef.GetItemCount(W05_MQ_101P_A_RepairTerminalKey) == 0
        playerRef.AddItem(W05_MQ_101P_A_RepairTerminalKey, 1, True)
    EndIf
    SetObjectiveCompleted(650)
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(600)
    SetObjectiveDisplayed(700)
    ; David's three tapes are created here rather than at 800 because 800 is
    ; set mid-scene by Rose's 0700 line and three ForceRefTo calls during a
    ; running scene can strip the speaker's scene package.
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    ObjectReference noteDesk = Alias_ArktosPharmaDeskTessa.GetReference()
    If Alias_DavidHolotapeOffice.GetReference() == None && noteDesk && (playerRef == None || playerRef.GetItemCount(W05_MQ_101P_A_DavidHolotapeNote) == 0)
        ObjectReference noteRef = noteDesk.PlaceAtMe(W05_MQ_101P_A_DavidHolotapeNote, 1, True)
        If noteRef
            noteRef.MoveTo(noteDesk, 0.0, 0.0, 56.0)
            Alias_DavidHolotapeOffice.ForceRefTo(noteRef)
        EndIf
    EndIf
    ObjectReference meetingMarker = Alias_ArktosPharmaDeskMarker.GetReference()
    If Alias_DavidHolotapeConferenceRoom.GetReference() == None && meetingMarker && (playerRef == None || playerRef.GetItemCount(W05_MQ_101P_A_DavidHolotapeMeeting) == 0)
        ObjectReference meetingRef = meetingMarker.PlaceAtMe(W05_MQ_101P_A_DavidHolotapeMeeting, 1, True)
        If meetingRef
            Alias_DavidHolotapeConferenceRoom.ForceRefTo(meetingRef)
        EndIf
    EndIf
    ObjectReference trashCan = Alias_ArktosPharmaTrashCan.GetReference()
    If Alias_DavidHolotapeControlRoom.GetReference() == None && trashCan && (playerRef == None || playerRef.GetItemCount(W05_MQ_101P_A_DavidHolotapeTessa) == 0)
        ObjectReference tessaRef = trashCan.PlaceAtMe(W05_MQ_101P_A_DavidHolotapeTessa, 1, True)
        If tessaRef
            Alias_DavidHolotapeControlRoom.ForceRefTo(tessaRef)
            trashCan.AddItem(tessaRef, 1, True)
        EndIf
    EndIf
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
    SetObjectiveCompleted(700)
    SetObjectiveDisplayed(800)
    ; Stage 805 carries the mid-quest XP/caps (B21:QuestRewards); FO76 granted it server side.
    If !IsStageDone(805)
        SetStage(805)
    EndIf
EndFunction

Function Fragment_Stage_0810_Item_00()
    SetObjectiveDisplayed(800, True, True)
EndFunction

Function Fragment_Stage_0820_Item_00()
    If !IsStageDone(900)
        SetStage(900)
    EndIf
EndFunction

Function Fragment_Stage_0830_Item_00()
    SetObjectiveDisplayed(800, True, True)
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(800)
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
    SetObjectiveCompleted(900)
    SetObjectiveDisplayed(950)
EndFunction

Function Fragment_Stage_0960_Item_00()
    SetObjectiveCompleted(950)
    W05_MQ_101P_A_QuestScript controller = (Self as Quest) as W05_MQ_101P_A_QuestScript
    If controller
        controller.FillRelayTowerSpeaker()
    EndIf
    If W05_MQ_101P_A_1000_DavidScene && !W05_MQ_101P_A_1000_DavidScene.IsPlaying()
        W05_MQ_101P_A_1000_DavidScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0970_Item_00()
    SetObjectiveDisplayed(970)
    Actor roseRef = None
    ReferenceAlias roseAlias = GetAlias(1) as ReferenceAlias
    If roseAlias
        roseRef = roseAlias.GetActorReference()
    EndIf
    If roseRef == None
        roseRef = RDR_Contact_Rose
    EndIf
    If roseRef && W05_MQ_101P_A_RoseFun_TopicInfo
        roseRef.Say(W05_MQ_101P_A_RoseFun_TopicInfo, None, True, Alias_currentPlayer.GetReference())
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(970)
    SetObjectiveDisplayed(1000)
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.SetValue(W05_MQ_101P_A_TopOfTheWorldValue, 1.0)
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
    SetObjectiveCompleted(1000)
    SetObjectiveDisplayed(1100)
    W05_MQ_101P_A_QuestScript controller = (Self as Quest) as W05_MQ_101P_A_QuestScript
    If controller == None
        Actor megRef = Alias_Meg.GetActorReference()
        If megRef
            megRef.Enable()
            megRef.EvaluatePackage()
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1110_Item_00()
    If !IsStageDone(1200)
        SetStage(1200)
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveCompleted(1100)
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
    SetObjectiveCompleted(1200)
    SetObjectiveDisplayed(1300)
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If playerRef
        playerRef.SetValue(W05_RaiderFactionAV, 1.0)
        playerRef.SetValue(Reputation_AllowHostileTier_AV_Crater, 0.0)
    EndIf
    Actor aldridgeRef = Alias_Aldridge.GetActorReference()
    If aldridgeRef
        aldridgeRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1400_Item_00()
    SetObjectiveCompleted(1300)
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
    SetObjectiveCompleted(1400)
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
    Int objectiveIndex = 1200
    While objectiveIndex <= 1500
        If IsObjectiveDisplayed(objectiveIndex)
            SetObjectiveCompleted(objectiveIndex)
        EndIf
        objectiveIndex += 100
    EndWhile
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
