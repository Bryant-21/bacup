Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0110_Item_00()
    ; Objective 110 reads the plan "from your inventory", but alias SignPlans is created
    ; at the SchematicSpawn marker. Move that alias ref into the player so the alias'
    ; OnRead handler still fires and sets stage 200.
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    ObjectReference playerObj = Alias_owningPlayer.GetReference()
    If playerRef
        ObjectReference plansRef = Alias_SignPlans.GetReference()
        If plansRef
            If plansRef.GetContainer() != playerObj
                playerRef.AddItem(plansRef, 1, True)
            EndIf
        ElseIf playerRef.GetItemCount(W05_MQ_002P_Radical_Recipe_Workshop_CraneRadioTransmitter) == 0
            playerRef.AddItem(W05_MQ_002P_Radical_Recipe_Workshop_CraneRadioTransmitter, 1, True)
        EndIf
    EndIf
    SetObjectiveDisplayed(110)
EndFunction

Function Fragment_Stage_0125_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef && W05_MQ_002P_Radical_DirectPlayerToAskAboutCamps
        playerRef.SetValue(W05_MQ_002P_Radical_DirectPlayerToAskAboutCamps, 1.0)
    EndIf
    SetObjectiveDisplayed(125)
EndFunction

Function Fragment_Stage_0130_Item_00()
    If GorgeJunkyardMapMarker
        GorgeJunkyardMapMarker.AddToMap(True)
    EndIf
    SetObjectiveDisplayed(130)
EndFunction

Function Fragment_Stage_0200_Item_00()
    ; COBJ W05_MQ_002P_Radical_workshop_co_CraneRadioTransmitter is gated on
    ; GetValue(PlayerLearnedSignRecipe) == 1, so the sign is unbuildable until this is set.
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef && W05_MQ_002P_Radical_PlayerLearnedSignRecipe
        playerRef.SetValue(W05_MQ_002P_Radical_PlayerLearnedSignRecipe, 1.0)
    EndIf
    SetObjectiveCompleted(110)
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0400_Item_00()
    ; Hand over the broadcast tape the sign produced; objective 350 and the radio
    ; terminal's menu-item handler both require it in the player's inventory, and
    ; stage 460 removes it from there again.
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    ObjectReference playerObj = Alias_owningPlayer.GetReference()
    If playerRef
        ObjectReference tapeRef = Alias_ConnectionTape.GetReference()
        If tapeRef && tapeRef.GetContainer() != playerObj
            playerRef.AddItem(tapeRef, 1, True)
        EndIf
    EndIf
    If W05_MQ_002P_Radical_0400_PlayerPowerSignForFirstTime
        W05_MQ_002P_Radical_0400_PlayerPowerSignForFirstTime.Start()
    EndIf
    SetObjectiveCompleted(125)
    SetObjectiveDisplayed(400)
    SetObjectiveDisplayed(350)
EndFunction

Function Fragment_Stage_0475_Item_00()
    SetObjectiveDisplayed(475)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveDisplayed(700)
EndFunction

Function Fragment_Stage_0998_Item_00()
    SetObjectiveDisplayed(998)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveDisplayed(1000)
EndFunction

Function Fragment_Stage_1020_Item_00()
    SetObjectiveDisplayed(1020)
EndFunction

Function Fragment_Stage_1030_Item_00()
    SetObjectiveDisplayed(1030)
EndFunction

Function Fragment_Stage_1210_Item_00()
    SetObjectiveDisplayed(1210)
EndFunction

Function Fragment_Stage_1310_Item_00()
    SetObjectiveDisplayed(1310)
EndFunction

Function Fragment_Stage_1320_Item_00()
    SetObjectiveDisplayed(1320)
EndFunction

Function Fragment_Stage_1600_Item_00()
    SetObjectiveDisplayed(1600)
EndFunction

Function Fragment_Stage_2000_Item_00()
    SetObjectiveDisplayed(2000)
EndFunction

Function Fragment_Stage_0140_Item_00()
    SetObjectiveCompleted(125)
EndFunction

Function Fragment_Stage_0505_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_002P_Radical_PlayerWasAJerkToFirstEnc, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0510_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_002P_Radical_FirstEncDismissed, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0610_Item_00()
    If W05_MQ_002P_Radical_600_GangerScene
        W05_MQ_002P_Radical_600_GangerScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0709_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_002P_Radical_PlayerKnowsRadicalsLocation, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0720_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_002P_Radical_SecondEncDismissedFast, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0736_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_002P_Radical_PlayerKnowsPassword, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1220_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_002P_Radical_PlayerKnowsPassword, 1.0)
    EndIf
    SetObjectiveCompleted(1210)
EndFunction

Function Fragment_Stage_1240_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_002P_Radical_PlayerKnowsPassword, 1.0)
    EndIf
    SetObjectiveCompleted(1210)
EndFunction

Function Fragment_Stage_1315_Item_00()
    If !IsStageDone(1320)
        SetStage(1320)
    EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_002P_Radical_SplitTreasureWithRoper, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1700_Item_00()
    SetObjectiveCompleted(1600)
    If !IsStageDone(2000)
        SetStage(2000)
    EndIf
EndFunction

Function Fragment_Stage_2115_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_001P_Wayward_PlayerNegotiatedBetterPrice_003, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0450_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_002P_Radical_PlayerConnectedRadioStation, 1.0)
    EndIf
    SetObjectiveCompleted(350)
    SetObjectiveCompleted(400)
    If W05_MQ_002P_Radical_0450_RadioSignConnected
        W05_MQ_002P_Radical_0450_RadioSignConnected.Start()
    EndIf
    If !IsStageDone(475)
        SetStage(475)
    EndIf
EndFunction

Function Fragment_Stage_1550_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.RemoveFromFaction(W05_RadicalEnemyFaction)
        playerRef.AddToFaction(W05_RadicalFriendFaction)
    EndIf
    Actor roperRef = Alias_Roper.GetActorReference()
    If roperRef
        roperRef.StopCombat()
        roperRef.EvaluatePackage()
    EndIf
    SetObjectiveCompleted(1010)
    SetObjectiveCompleted(1011)
    If !IsStageDone(2000)
        SetStage(2000)
    EndIf
EndFunction

Function Fragment_Stage_8950_Item_00()
    ; Completion must not depend on the successor Story Manager node accepting an
    ; event in the same frame. Stage 9000 owns the B21 reward/XP notification.
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_0001_Item_00()
    ; FO76's debug-material grant was server-authored and has no local gameplay dependency.
    Return
EndFunction

Function Fragment_Stage_0002_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef && Stage2DebugMarker
        playerRef.MoveTo(Stage2DebugMarker)
    EndIf
EndFunction

Function Fragment_Stage_0005_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef && IntroTeleportTarget
        playerRef.MoveTo(IntroTeleportTarget)
    EndIf
EndFunction

Function Fragment_Stage_0006_Item_00()
    ; FO76 used an instance-owner callback here. The local hostile entry converges on the normal attack stage.
    If !IsStageDone(1575)
        SetStage(1575)
    EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef && W05_MQ_002P_Radical_Checkpoint
        playerRef.SetValue(W05_MQ_002P_Radical_Checkpoint, 1.0)
    EndIf
    If !IsStageDone(100)
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(110)
EndFunction

Function Fragment_Stage_0160_Item_00()
    SetObjectiveDisplayed(125, True, True)
EndFunction

Function Fragment_Stage_0270_Item_00()
    SetObjectiveCompleted(130)
    SetObjectiveCompleted(200)
    If !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0460_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    ObjectReference tapeRef = Alias_ConnectionTape.GetReference()
    If playerRef && tapeRef
        playerRef.RemoveItem(tapeRef, 1, True)
    EndIf
EndFunction

Function Fragment_Stage_0498_Item_00()
    If W05_MQ_002P_Radical_0449_HunterShoutScene
        W05_MQ_002P_Radical_0449_HunterShoutScene.Start()
    EndIf
    If !IsStageDone(501)
        SetStage(501)
    EndIf
EndFunction

Function Fragment_Stage_0501_Item_00()
    If W05_MQ_002P_Radical_0500_TreasureHunter_Scene
        W05_MQ_002P_Radical_0500_TreasureHunter_Scene.Start()
    EndIf
    If !IsStageDone(502)
        SetStage(502)
    EndIf
EndFunction

Function Fragment_Stage_0502_Item_00()
    Actor visitorRef = Alias_FirstEncNPC.GetActorReference()
    If visitorRef && W05_MQ_002_FirstEncGreeting
        visitorRef.Say(W05_MQ_002_FirstEncGreeting)
    EndIf
EndFunction

Function Fragment_Stage_0504_Item_00()
    Actor visitorRef = Alias_FirstEncNPC.GetActorReference()
    If visitorRef
        visitorRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0511_Item_00()
    Actor visitorRef = Alias_FirstEncNPC.GetActorReference()
    If visitorRef
        If W05_MQ_002_FirstEncFlees
            visitorRef.Say(W05_MQ_002_FirstEncFlees)
        EndIf
        visitorRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0515_Item_00()
    If W05_MQ_002P_Radical_0500_TreasureHunter_Scene
        W05_MQ_002P_Radical_0500_TreasureHunter_Scene.Stop()
    EndIf
    If !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0525_Item_00()
    If !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0707_Item_00()
    Actor ganger01 = Alias_RadGanger01.GetActorReference()
    Actor ganger02 = Alias_RadGanger02.GetActorReference()
    If ganger01
        ganger01.EvaluatePackage()
    EndIf
    If ganger02
        ganger02.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0710_Item_00()
    If !IsStageDone(746)
        SetStage(746)
    EndIf
EndFunction

Function Fragment_Stage_0725_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_002P_Radical_PlayerKnowsRadicalsLocation, 1.0)
    EndIf
    If !IsStageDone(799)
        SetStage(799)
    EndIf
EndFunction

Function Fragment_Stage_0735_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.RemoveFromFaction(W05_RadicalEnemyFaction)
        playerRef.AddToFaction(W05_RadicalFriendFaction)
        playerRef.SetValue(W05_MQ_002P_Radical_RadicalFriendValue, 1.0)
        playerRef.SetValue(W05_MQ_002P_Radical_PlayerKnowsRadicalsLocation, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0745_Item_00()
    ; The controller also owns this timer; the fragment fallback keeps the local route live if its stage callback is unavailable.
    Utility.Wait(3.0)
    If !IsStageDone(746)
        SetStage(746)
    EndIf
EndFunction

Function Fragment_Stage_0746_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    Actor ganger01 = Alias_RadGanger01.GetActorReference()
    Actor ganger02 = Alias_RadGanger02.GetActorReference()
    If playerRef
        playerRef.AddToFaction(W05_RadicalEnemyFaction)
    EndIf
    If ganger01 && playerRef
        ganger01.AddToFaction(PlayerEnemyFaction)
        ganger01.StartCombat(playerRef)
    EndIf
    If ganger02 && playerRef
        ganger02.AddToFaction(PlayerEnemyFaction)
        ganger02.StartCombat(playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0760_Item_00()
    If W05_MQ_002P_Radical_600_GangerScene
        W05_MQ_002P_Radical_600_GangerScene.Stop()
    EndIf
    Actor ganger01 = Alias_RadGanger01.GetActorReference()
    Actor ganger02 = Alias_RadGanger02.GetActorReference()
    If ganger01
        ganger01.EvaluatePackage()
    EndIf
    If ganger02
        ganger02.EvaluatePackage()
    EndIf
    If !IsStageDone(746) && !IsStageDone(799)
        SetStage(799)
    EndIf
EndFunction

Function Fragment_Stage_0764_Item_00()
    If !IsStageDone(746)
        SetStage(746)
    EndIf
EndFunction

Function Fragment_Stage_0765_Item_00()
    If IsStageDone(766) && !IsStageDone(799)
        SetStage(799)
    EndIf
EndFunction

Function Fragment_Stage_0766_Item_00()
    If IsStageDone(765) && !IsStageDone(799)
        SetStage(799)
    EndIf
EndFunction

Function Fragment_Stage_0799_Item_00()
    SetObjectiveCompleted(700)
    If WVMapMarker
        WVMapMarker.AddToMap(False)
    EndIf
    If !IsStageDone(998)
        SetStage(998)
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveCompleted(998)
    SetObjectiveCompleted(1000)
    SetObjectiveDisplayed(1010)
    SetObjectiveDisplayed(1011)
    SetObjectiveDisplayed(1020)
    SetObjectiveDisplayed(1030)
    SetObjectiveDisplayed(1040)
EndFunction

Function Fragment_Stage_1101_Item_00()
    If W05_MQ_002P_Radical_TutorialFastTravel
        W05_MQ_002P_Radical_TutorialFastTravel.Show()
    EndIf
EndFunction

Function Fragment_Stage_1140_Item_00()
    SetObjectiveCompleted(1040)
EndFunction

Function Fragment_Stage_1290_Item_00()
    SetObjectiveCompleted(1020)
    SetObjectiveCompleted(1210)
EndFunction

Function Fragment_Stage_1350_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    ObjectReference eggRef = Alias_Egg.GetReference()
    If playerRef && eggRef
        playerRef.RemoveItem(eggRef, 1, True)
    EndIf
    SetObjectiveCompleted(1030)
    SetObjectiveCompleted(1310)
    SetObjectiveCompleted(1320)
EndFunction

Function Fragment_Stage_1575_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    Actor roperRef = Alias_Roper.GetActorReference()
    Actor jackyRef = Alias_Jacky.GetActorReference()
    If playerRef
        playerRef.AddToFaction(W05_RadicalEnemyFaction)
    EndIf
    If roperRef && playerRef
        roperRef.AddToFaction(W05_Radical_RoperEnemyFaction)
        roperRef.StartCombat(playerRef)
    EndIf
    If jackyRef && playerRef
        jackyRef.AddToFaction(PlayerEnemyFaction)
        jackyRef.StartCombat(playerRef)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef && W05_MQ_002P_Radical_PlayerCompleted002p
        playerRef.SetValue(W05_MQ_002P_Radical_PlayerCompleted002p, 1.0)
    EndIf
    SetObjectiveCompleted(2000)
    CompleteQuest()
EndFunction
