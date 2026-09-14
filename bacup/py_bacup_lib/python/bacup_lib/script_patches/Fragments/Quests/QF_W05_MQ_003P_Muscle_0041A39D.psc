Function Fragment_Stage_0001_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef && DebugMarker01
        playerRef.MoveTo(DebugMarker01)
    EndIf
EndFunction

Function Fragment_Stage_0002_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef && DebugMarker02
        playerRef.MoveTo(DebugMarker02)
    EndIf
EndFunction

Function Fragment_Stage_0003_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef && DebugMarker03
        playerRef.MoveTo(DebugMarker03)
    EndIf
EndFunction

Function Fragment_Stage_0004_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef && DebugMarker04
        playerRef.MoveTo(DebugMarker04)
    EndIf
EndFunction

Function Fragment_Stage_0005_Item_00()
    If !IsStageDone(1000)
        SetStage(1000)
    EndIf
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef && DebugMarker05
        playerRef.MoveTo(DebugMarker05)
    EndIf
EndFunction

Function Fragment_Stage_0006_Item_00()
    If !IsStageDone(1200)
        SetStage(1200)
    EndIf
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef && DebugMarker06
        playerRef.MoveTo(DebugMarker06)
    EndIf
EndFunction

Function Fragment_Stage_0007_Item_00()
    If !IsStageDone(1300)
        SetStage(1300)
    EndIf
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef && IntroTeleportTarget
        playerRef.MoveTo(IntroTeleportTarget)
    EndIf
EndFunction

Function Fragment_Stage_0008_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef && IntroTeleportTarget
        playerRef.MoveTo(IntroTeleportTarget)
    EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_003P_Muscle_PlayerStartedQuest, 1.0)
    EndIf
    If W05_MQ_003P_Muscle_Radio_QuestStartKeyword
        W05_MQ_003P_Muscle_Radio_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
    If W05_MQ_003P_Muscle_0100_StartScene
        W05_MQ_003P_Muscle_0100_StartScene.Start()
    ElseIf !IsStageDone(150)
        SetStage(150)
    EndIf
EndFunction

Function Fragment_Stage_0103_Item_00()
    If W05_MQ_003P_Muscle_0100_StartScene && !W05_MQ_003P_Muscle_0100_StartScene.IsPlaying()
        W05_MQ_003P_Muscle_0100_StartScene.Start()
    ElseIf !IsStageDone(150)
        SetStage(150)
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(125)
    If W05_Tutorial_PipBoyRadio
        W05_Tutorial_PipBoyRadio.Show()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(125)
    SetObjectiveCompleted(150)
    SetObjectiveDisplayed(200)
    If TEMP_W05_MQ_003P_ArriveAtFightPOI
        TEMP_W05_MQ_003P_ArriveAtFightPOI.Show()
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(200)
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(300)
    SetObjectiveDisplayed(400)
    If W05_MQ_003P_Muscle_0400_SolAttactScene
        W05_MQ_003P_Muscle_0400_SolAttactScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(400)
    SetObjectiveDisplayed(500)
    If IsStageDone(800) && !IsStageDone(900)
        SetStage(900)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(500)
    SetObjectiveDisplayed(600)
    If W05_MQ_003P_Muscle_0600_PollyAttractScene
        W05_MQ_003P_Muscle_0600_PollyAttractScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(600)
    SetObjectiveDisplayed(700)
    If !IsStageDone(710)
        SetStage(710)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(350)
    SetObjectiveCompleted(400)
    SetObjectiveCompleted(500)
    SetObjectiveCompleted(600)
    SetObjectiveCompleted(700)
    SetObjectiveDisplayed(1000)
EndFunction

Function Fragment_Stage_1020_Item_00()
    SetObjectiveDisplayed(1020)
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveCompleted(1000)
    SetObjectiveCompleted(1020)
    SetObjectiveDisplayed(1100)
EndFunction

Function Fragment_Stage_1150_Item_00()
    SetObjectiveCompleted(1100)
    SetObjectiveDisplayed(1150)
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef && playerRef.GetItemCount(W05_MQ_003P_Muscle_PollyAssaultronHead) > 0
        playerRef.RemoveItem(W05_MQ_003P_Muscle_PollyAssaultronHead, 1, True)
    EndIf
    If playerRef
        playerRef.SetValue(W05_MQ_003P_Muscle_EmptyJugSwap, 1.0)
    EndIf
    ObjectReference soundMarker = Alias_JugSwapSoundMarker.GetReference()
    If QST003PHeadJug && soundMarker
        QST003PHeadJug.Play(soundMarker)
    EndIf
    If W05_MQ_003P_Muscle_1150_PollyScene
        W05_MQ_003P_Muscle_1150_PollyScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveCompleted(1150)
    SetObjectiveDisplayed(1200)
    SetObjectiveDisplayed(1205)
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        ObjectReference signalRef = Alias_SignalBeacon.GetReference()
        If signalRef && playerRef.GetItemCount(signalRef) == 0
            playerRef.AddItem(signalRef, 1, True)
        EndIf
        If W05_MQ_003P_Muscle_DnDAccessCard && playerRef.GetItemCount(W05_MQ_003P_Muscle_DnDAccessCard) == 0
            playerRef.AddItem(W05_MQ_003P_Muscle_DnDAccessCard, 1, True)
        EndIf
        playerRef.SetValue(W05_MQ_003P_Muscle_PlayerCanAccessDuncan, 1.0)
    EndIf
    If W05_MQ_003P_Muscle_DuncanNDuncanQuest_Interior && !W05_MQ_003P_Muscle_DuncanNDuncanQuest_Interior.IsRunning()
        W05_MQ_003P_Muscle_DuncanNDuncanQuest_Interior.Start()
    EndIf
EndFunction

Function Fragment_Stage_1205_Item_00()
    SetObjectiveDisplayed(1205)
EndFunction

Function Fragment_Stage_1300_Item_00()
    SetObjectiveCompleted(1200)
    SetObjectiveCompleted(1205)
    SetObjectiveDisplayed(1300)
EndFunction

Function Fragment_Stage_1390_Item_00()
    SetObjectiveDisplayed(1390)
EndFunction

Function Fragment_Stage_0410_Item_00()
    Actor solRef = Alias_Sol.GetActorReference()
    If solRef
        solRef.RemoveFromFaction(CaptiveFaction)
        solRef.AddToFaction(W05_CrimeTheWayward)
    EndIf
EndFunction

Function Fragment_Stage_0415_Item_00()
    Actor solRef = Alias_Sol.GetActorReference()
    If solRef
        solRef.ResetHealthAndLimbs()
    EndIf
EndFunction

Function Fragment_Stage_0450_Item_00()
    Actor solRef = Alias_Sol.GetActorReference()
    If solRef
        solRef.ChangeAnimArchetype(AnimArchetypeDepressed)
        solRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0476_Item_00()
    If !IsStageDone(470)
        SetStage(470)
    EndIf
    Actor solRef = Alias_Sol.GetActorReference()
    If solRef
        solRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1050_Item_00()
    If W05_MQ_003P_Muscle_1010b_PollyEquipped
        W05_MQ_003P_Muscle_1010b_PollyEquipped.Start()
    EndIf
EndFunction

Function Fragment_Stage_0499_Item_00()
    If W05_MQ_003P_Muscle_0500_SolExitsScene
        W05_MQ_003P_Muscle_0500_SolExitsScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0550_Item_00()
    ObjectReference gauleyMarker = Alias_Sol_GauleyMine_EnableMarker.GetReference()
    ObjectReference waywardMarker = Alias_Sol_Wayward_EnableMarker.GetReference()
    If gauleyMarker
        gauleyMarker.Disable()
    EndIf
    If waywardMarker
        waywardMarker.Enable()
    EndIf
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_003P_Muscle_GauleyMineSolOn_Permanent, 0.0)
        playerRef.SetValue(W05_MQ_003P_Muscle_WaywardSolOn, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0710_Item_00()
    W05_MQ_003P_Muscle_ToggleMusicOverride.SetValue(1.0)
    ; Must widen to Quest before narrowing: DefaultQuestEncounterWaveScript is a
    ; sibling script on this same quest, not an ancestor of this one, so a direct
    ; "Self as ..." is rejected by the stock compiler and yields None at runtime —
    ; which would silently skip both Scorched waves via the ElseIf fallback.
    DefaultQuestEncounterWaveScript encounterController = (Self as Quest) as DefaultQuestEncounterWaveScript
    If encounterController
        encounterController.StartLocalEncounterWave(0)
    ElseIf !IsStageDone(715)
        SetStage(715)
    EndIf
EndFunction

Function Fragment_Stage_0715_Item_00()
    RefCollectionAlias spawnedScorched = GetAlias(17) as RefCollectionAlias
    If (spawnedScorched == None || spawnedScorched.GetCount() == 0) && !IsStageDone(725)
        SetStage(725)
    EndIf
EndFunction

Function Fragment_Stage_0725_Item_00()
    DefaultQuestEncounterWaveScript encounterController = (Self as Quest) as DefaultQuestEncounterWaveScript
    If encounterController
        encounterController.StartLocalEncounterWave(1)
    ElseIf !IsStageDone(800)
        SetStage(800)
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    W05_MQ_003P_Muscle_ToggleMusicOverride.SetValue(0.0)
    SetObjectiveCompleted(700)
    If IsStageDone(500)
        If !IsStageDone(900)
            SetStage(900)
        EndIf
    Else
        SetObjectiveDisplayed(350)
        SetObjectiveDisplayed(400)
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    W05_MQ_003P_Muscle_ToggleMusicOverride.SetValue(0.0)
    ObjectReference pollyMarker = Alias_Polly_GauleyMine_EnableMarker.GetReference()
    ObjectReference solMarker = Alias_Sol_GauleyMine_EnableMarker.GetReference()
    If pollyMarker
        pollyMarker.Disable()
    EndIf
    If solMarker
        solMarker.Disable()
    EndIf
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_003P_Muscle_GauleyMineComplete, 1.0)
        playerRef.SetValue(W05_MQ_003P_Muscle_GauleyMinePollyOn_Permanent, 0.0)
        playerRef.SetValue(W05_MQ_003P_Muscle_GauleyMineSolOn_Permanent, 0.0)
        playerRef.SetValue(W05_MQ_003P_Muscle_WaywardSolOn, 1.0)
        playerRef.SetValue(W05_MQ_003P_Muscle_Wayward_PollyHeadOn, 1.0)
    EndIf
    If !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_0999_Item_00()
    If !IsStageDone(998)
        SetStage(998)
    EndIf
    If !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_1005_Item_00()
    If W05_MQ_003P_Muscle_1005_ReturnScene
        W05_MQ_003P_Muscle_1005_ReturnScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1015_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef && W05_MQ_003P_Muscle_PollyAssaultronHead
        If playerRef.GetItemCount(W05_MQ_003P_Muscle_PollyAssaultronHead) > 0
            playerRef.EquipItem(W05_MQ_003P_Muscle_PollyAssaultronHead, False, True)
            playerRef.DrawWeapon()
        EndIf
    EndIf
    SetObjectiveDisplayed(1020)
EndFunction

Function Fragment_Stage_1025_Item_00()
    SetObjectiveCompleted(1020)
    If !IsStageDone(1050)
        SetStage(1050)
    EndIf
EndFunction

Function Fragment_Stage_1224_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.AddItem(W05_MQ_003P_Muscle_AssaultronRoomCard, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_1225_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.AddItem(W05_MQ_003P_Muscle_HandyRoomKey, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_1226_Item_00()
    Actor skinnerRef = Alias_Skinner.GetActorReference()
    If skinnerRef
        skinnerRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1229_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        If Caps001 && playerRef.GetItemCount(Caps001) > 0
            playerRef.RemoveItem(Caps001, 1, False)
        EndIf
        If W05_MQ_003P_Muscle_AssaultronRoomCard && playerRef.GetItemCount(W05_MQ_003P_Muscle_AssaultronRoomCard) == 0
            playerRef.AddItem(W05_MQ_003P_Muscle_AssaultronRoomCard, 1, False)
        EndIf
    EndIf
    If !IsStageDone(1233)
        SetStage(1233)
    EndIf
EndFunction

Function Fragment_Stage_1230_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_003P_Muscle_PlayerKilledSkinner, 1.0)
        If W05_MQ_003P_Muscle_ProtectronRoomKey && playerRef.GetItemCount(W05_MQ_003P_Muscle_ProtectronRoomKey) == 0
            playerRef.AddItem(W05_MQ_003P_Muscle_ProtectronRoomKey, 1, True)
        EndIf
        If W05_MQ_003P_Muscle_HandyRoomKey && playerRef.GetItemCount(W05_MQ_003P_Muscle_HandyRoomKey) == 0
            playerRef.AddItem(W05_MQ_003P_Muscle_HandyRoomKey, 1, True)
        EndIf
        If W05_MQ_003P_Muscle_AssaultronRoomKey && playerRef.GetItemCount(W05_MQ_003P_Muscle_AssaultronRoomKey) == 0
            playerRef.AddItem(W05_MQ_003P_Muscle_AssaultronRoomKey, 1, True)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1232_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        If W05_MQ_003P_Muscle_AssaultronRoomKey && playerRef.GetItemCount(W05_MQ_003P_Muscle_AssaultronRoomKey) == 0
            playerRef.AddItem(W05_MQ_003P_Muscle_AssaultronRoomKey, 1, False)
        EndIf
        playerRef.SetValue(W05_MQ_003P_Muscle_PlayerCanAccessDuncan, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1240_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef && playerRef.GetValue(W05_MQ_003P_Muscle_PlayerGotCapsFromDuchess) < 1.0
        Int rewardCaps = 200
        If IsStageDone(1175)
            rewardCaps = 300
        EndIf
        playerRef.AddItem(Caps001, rewardCaps, False)
        playerRef.SetValue(W05_MQ_003P_Muscle_PlayerGotCapsFromDuchess, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1251_Item_00()
    If !IsStageDone(1260)
        SetStage(1260)
    EndIf
EndFunction

Function Fragment_Stage_1270_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_003P_Muscle_PollyBodyChoiceIndex, 1.0)
        playerRef.SetValue(W05_MQ_003P_Muscle_DnDBodyChoiceIndex, 1.0)
    EndIf
    If !IsStageDone(1300)
        SetStage(1300)
    EndIf
EndFunction

Function Fragment_Stage_1275_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_003P_Muscle_PollyBodyChoiceIndex, 2.0)
        playerRef.SetValue(W05_MQ_003P_Muscle_DnDBodyChoiceIndex, 2.0)
    EndIf
    If !IsStageDone(1300)
        SetStage(1300)
    EndIf
EndFunction

Function Fragment_Stage_1280_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_003P_Muscle_PollyBodyChoiceIndex, 3.0)
        playerRef.SetValue(W05_MQ_003P_Muscle_DnDBodyChoiceIndex, 3.0)
    EndIf
    If !IsStageDone(1300)
        SetStage(1300)
    EndIf
EndFunction

Function Fragment_Stage_1310_Item_00()
    If W05_MQ_003P_Muscle_1310_ReturnScene
        W05_MQ_003P_Muscle_1310_ReturnScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1311_Item_00()
    If W05_MQ_003P_Muscle_1311_RadioScene
        W05_MQ_003P_Muscle_1311_RadioScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1312_Item_00()
    If W05_MQ_003P_Muscle_1311_RadioScene
        W05_MQ_003P_Muscle_1311_RadioScene.Stop()
    EndIf
EndFunction

Function Fragment_Stage_1320_Item_00()
    If W05_MQ_003P_Muscle_1310_ReturnScene
        W05_MQ_003P_Muscle_1310_ReturnScene.Stop()
    EndIf
    If W05_MQ_003P_Muscle_1320_PlayerReturnsScene
        W05_MQ_003P_Muscle_1320_PlayerReturnsScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1325_Item_00()
    SetObjectiveCompleted(1300)
    SetObjectiveDisplayed(1315)
EndFunction

Function Fragment_Stage_1321_Item_00()
    Actor pollyRef = Alias_Polly_Wayward.GetActorReference()
    If pollyRef
        pollyRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
    SetObjectiveCompleted(1315)
    SetObjectiveCompleted(1390)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
    If W05_MQ_004P_Crane_QuestStartKeyword
        W05_MQ_004P_Crane_QuestStartKeyword.SendStoryEvent()
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_Wayward_JideDialogue_003PComplete, 1.0)
        playerRef.SetValue(W05_MQ_004P_ChangeWaywardStates, 1.0)
        Potion duchessDram = Game.GetFormFromFile(0x00572D17, "SeventySix.esm") as Potion
        If duchessDram
            playerRef.AddItem(duchessDram, 2, False)
        EndIf
    EndIf
    CompleteAllObjectives()
EndFunction

Function Fragment_Stage_10000_Item_00()
    W05_MQ_003P_Muscle_ToggleMusicOverride.SetValue(0.0)
    If W05_MQ_003P_Muscle_0100_StartScene
        W05_MQ_003P_Muscle_0100_StartScene.Stop()
    EndIf
    If W05_MQ_003P_Muscle_0400_SolAttactScene
        W05_MQ_003P_Muscle_0400_SolAttactScene.Stop()
    EndIf
    If W05_MQ_003P_Muscle_0600_PollyAttractScene
        W05_MQ_003P_Muscle_0600_PollyAttractScene.Stop()
    EndIf
    If W05_MQ_003P_Muscle_1005_ReturnScene
        W05_MQ_003P_Muscle_1005_ReturnScene.Stop()
    EndIf
    If W05_MQ_003P_Muscle_1010b_PollyEquipped
        W05_MQ_003P_Muscle_1010b_PollyEquipped.Stop()
    EndIf
    If W05_MQ_003P_Muscle_1150_PollyScene
        W05_MQ_003P_Muscle_1150_PollyScene.Stop()
    EndIf
    If W05_MQ_003P_Muscle_1310_ReturnScene
        W05_MQ_003P_Muscle_1310_ReturnScene.Stop()
    EndIf
    If W05_MQ_003P_Muscle_1311_RadioScene
        W05_MQ_003P_Muscle_1311_RadioScene.Stop()
    EndIf
    If W05_MQ_003P_Muscle_1320_PlayerReturnsScene
        W05_MQ_003P_Muscle_1320_PlayerReturnsScene.Stop()
    EndIf
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef && playerRef.GetItemCount(W05_MQ_003P_Muscle_PollyAssaultronHead) > 0
        playerRef.RemoveItem(W05_MQ_003P_Muscle_PollyAssaultronHead, 1, True)
    EndIf
    If W05_MQ_003P_Muscle_DuncanNDuncanQuest_Interior && W05_MQ_003P_Muscle_DuncanNDuncanQuest_Interior.IsRunning()
        W05_MQ_003P_Muscle_DuncanNDuncanQuest_Interior.Stop()
    EndIf
EndFunction
