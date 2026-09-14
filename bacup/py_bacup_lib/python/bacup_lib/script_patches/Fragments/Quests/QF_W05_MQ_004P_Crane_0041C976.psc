Function Fragment_Stage_0001_Item_00()
    If DebugMarker01
        Game.GetPlayer().MoveTo(DebugMarker01)
    EndIf
    If !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0002_Item_00()
    If !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0003_Item_00()
    If DebugMarker03
        Game.GetPlayer().MoveTo(DebugMarker03)
    EndIf
    If !IsStageDone(1150)
        SetStage(1150)
    EndIf
EndFunction

Function Fragment_Stage_0004_Item_00()
    Actor duchessRef = Alias_Duchess.GetActorReference()
    If duchessRef && TeleportPoint
        duchessRef.MoveTo(TeleportPoint)
    EndIf
    ObjectReference waywardDoor = Alias_WaywardDoor.GetReference()
    If waywardDoor
        waywardDoor.Unlock()
    EndIf
EndFunction

Function Fragment_Stage_0005_Item_00()
    Actor roperRef = Alias_Roper.GetActorReference()
    If roperRef
        roperRef.Enable()
        roperRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0006_Item_00()
    If DebugMarker02
        Game.GetPlayer().MoveTo(DebugMarker02)
    EndIf
    If !IsStageDone(100)
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
    If !IsStageDone(100)
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    ObjectReference tokenRef = Alias_Token.GetReference()
    If tokenRef
        If IsStageDone(1010)
            tokenRef.Disable()
        Else
            tokenRef.Enable()
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
    If W05_MQ_004P_Crane_0100_StartScene && !W05_MQ_004P_Crane_0100_StartScene.IsPlaying()
        W05_MQ_004P_Crane_0100_StartScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0102_Item_00()
    Actor craneRef = Alias_Crane.GetActorReference()
    If craneRef
        craneRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0103_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef && playerRef.GetValue(W05_MQ_004P_PlayerStartedQuestOnce) < 1.0 && W05_MQ_004P_Crane_0100a_StartScene
        W05_MQ_004P_Crane_0100a_StartScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0105_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_004P_PlayerStartedQuestOnce, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0108_Item_00()
    Actor craneRef = Alias_Crane.GetActorReference()
    If craneRef
        craneRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0109_Item_00()
    Actor solRef = Alias_Sol.GetActorReference()
    If solRef
        solRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0110_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(110)
EndFunction

Function Fragment_Stage_0111_Item_00()
    SetObjectiveCompleted(110)
    SetObjectiveDisplayed(111)
EndFunction

Function Fragment_Stage_0112_Item_00()
    Actor solRef = Alias_Sol.GetActorReference()
    If solRef
        solRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0125_Item_00()
    Actor solRef = Alias_Sol.GetActorReference()
    If solRef
        solRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(111)
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_0210_Item_00()
    ObjectReference mapRef = Alias_Map.GetReference()
    ObjectReference keycardRef = Alias_Keycard.GetReference()
    If mapRef
        mapRef.Enable()
    EndIf
    If keycardRef
        keycardRef.Enable()
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(111)
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0301_Item_00()
    Actor solRef = Alias_Sol.GetActorReference()
    Actor craneRef = Alias_Crane.GetActorReference()
    If solRef && craneRef && !craneRef.IsDead()
        solRef.StartCombat(craneRef)
    EndIf
EndFunction

Function Fragment_Stage_0310_Item_00()
    Actor solRef = Alias_Sol.GetActorReference()
    If solRef
        solRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0399_Item_00()
    SetObjectiveCompleted(300)
    SetObjectiveDisplayed(399)
    If W05_MQ_004P_Crane_0399_PlayerKilledCraneEarly && !W05_MQ_004P_Crane_0399_PlayerKilledCraneEarly.IsPlaying()
        W05_MQ_004P_Crane_0399_PlayerKilledCraneEarly.Start()
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(300)
    If W05_MQ_004P_Crane_0400_MomentOfSilenceScene && !W05_MQ_004P_Crane_0400_MomentOfSilenceScene.IsPlaying()
        W05_MQ_004P_Crane_0400_MomentOfSilenceScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0401_Item_00()
    SetObjectiveDisplayed(399)
    Actor duchessRef = Alias_Duchess.GetActorReference()
    If duchessRef
        duchessRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0495_Item_00()
    Actor duchessRef = Alias_Duchess.GetActorReference()
    Actor solRef = Alias_Sol.GetActorReference()
    If duchessRef
        duchessRef.EvaluatePackage()
    EndIf
    If solRef
        solRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(300)
    SetObjectiveCompleted(399)
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_0600_Item_00()
    If IsStageDone(650) && !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0650_Item_00()
    If IsStageDone(600) && !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(500)
    SetObjectiveDisplayed(700)
EndFunction

Function Fragment_Stage_0701_Item_00()
    SetObjectiveCompleted(500)
    SetObjectiveDisplayed(700)
EndFunction

Function Fragment_Stage_0702_Item_00()
    ObjectReference tokenRef = Alias_Token.GetReference()
    If tokenRef && !IsStageDone(1010)
        tokenRef.Enable()
    EndIf
EndFunction

Function Fragment_Stage_0703_Item_00()
    Actor duchessRef = Alias_Duchess.GetActorReference()
    Actor solRef = Alias_Sol.GetActorReference()
    If duchessRef
        duchessRef.EvaluatePackage()
    EndIf
    If solRef
        solRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0704_Item_00()
    Actor duchessRef = Alias_Duchess.GetActorReference()
    Actor solRef = Alias_Sol.GetActorReference()
    If duchessRef
        duchessRef.EvaluatePackage()
    EndIf
    If solRef
        solRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0710_Item_00()
    Actor duchessRef = Alias_Duchess.GetActorReference()
    Actor solRef = Alias_Sol.GetActorReference()
    If duchessRef
        duchessRef.EvaluatePackage()
    EndIf
    If solRef
        solRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0750_Item_00()
    SetObjectiveDisplayed(750)
EndFunction

Function Fragment_Stage_0760_Item_00()
    SetObjectiveCompleted(750)
    SetObjectiveDisplayed(760)
EndFunction

Function Fragment_Stage_0765_Item_00()
    SetObjectiveCompleted(750)
    SetObjectiveCompleted(760)
    ObjectReference cacheDoor = Alias_CacheDoor.GetReference()
    If cacheDoor
        cacheDoor.Unlock()
        cacheDoor.SetOpen(True)
    EndIf
    If !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_0775_Item_00()
    SetObjectiveCompleted(700)
    SetObjectiveDisplayed(800)
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveDisplayed(800)
EndFunction

Function Fragment_Stage_0820_Item_00()
    SetObjectiveDisplayed(800)
    ObjectReference cacheDoor = Alias_CacheDoor.GetReference()
    If cacheDoor
        cacheDoor.Unlock()
    EndIf
EndFunction

Function Fragment_Stage_0830_Item_00()
    SetObjectiveCompleted(700)
    SetObjectiveDisplayed(800)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(700)
    SetObjectiveCompleted(750)
    SetObjectiveCompleted(760)
    SetObjectiveCompleted(800)
    SetObjectiveDisplayed(1000)
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    ActorValue cacheOpenedValue = Game.GetFormFromFile(0x005911D2, "SeventySix.esm") as ActorValue
    ObjectReference firstCacheLight = Game.GetFormFromFile(0x005911D1, "SeventySix.esm") as ObjectReference
    If playerRef && cacheOpenedValue
        playerRef.SetValue(cacheOpenedValue, 1.0)
        If firstCacheLight && firstCacheLight.IsDisabled()
            firstCacheLight.Enable()
        EndIf
    EndIf
    ObjectReference cacheDoor = Alias_CacheDoor.GetReference()
    If cacheDoor
        cacheDoor.Unlock()
        cacheDoor.SetOpen(True)
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveCompleted(1000)
    SetObjectiveDisplayed(1100)
EndFunction

Function Fragment_Stage_1105_Item_00()
    ObjectReference markerRef = Alias_WaywardPowerUpMarkers.GetReference()
    If markerRef
        markerRef.Enable()
    EndIf
EndFunction

Function Fragment_Stage_1150_Item_00()
    Actor roperRef = Alias_Roper.GetActorReference()
    If roperRef && !roperRef.IsDead()
        roperRef.Enable()
        roperRef.EvaluatePackage()
    ElseIf !IsStageDone(1300)
        SetStage(1300)
    EndIf
EndFunction

Function Fragment_Stage_1170_Item_00()
    If FlatwoodsMapMarker
        FlatwoodsMapMarker.AddToMap(False)
    EndIf
EndFunction

Function Fragment_Stage_1180_Item_00()
    If MorgantownAirportMapMarker
        MorgantownAirportMapMarker.AddToMap(False)
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveCompleted(1100)
    SetObjectiveDisplayed(1200)
    If W05_MQ_004P_Crane_1200_RoperScene && !W05_MQ_004P_Crane_1200_RoperScene.IsPlaying()
        W05_MQ_004P_Crane_1200_RoperScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1220_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_004P_Crane_RoperResolutionIndex, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1221_Item_00()
    SetObjectiveCompleted(1200)
    SetObjectiveDisplayed(1230)
EndFunction

Function Fragment_Stage_1230_Item_00()
    SetObjectiveCompleted(1200)
    SetObjectiveDisplayed(1230)
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    Int radicalIndex = 0
    While playerRef && radicalIndex < Alias_Radicals.GetCount()
        Actor radicalRef = Alias_Radicals.GetAt(radicalIndex) as Actor
        If radicalRef && !radicalRef.IsDead()
            radicalRef.StartCombat(playerRef)
        EndIf
        radicalIndex += 1
    EndWhile
EndFunction

Function Fragment_Stage_1235_Item_00()
    SetObjectiveDisplayed(1230)
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    Actor roperRef = Alias_Roper.GetActorReference()
    If playerRef && roperRef && !roperRef.IsDead()
        roperRef.StartCombat(playerRef)
    EndIf
EndFunction

Function Fragment_Stage_1240_Item_00()
    SetObjectiveCompleted(1200)
    If !IsStageDone(1300)
        SetStage(1300)
    EndIf
EndFunction

Function Fragment_Stage_1242_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_004P_Crane_RoperResolutionIndex, 2.0)
    EndIf
EndFunction

Function Fragment_Stage_1243_Item_00()
    ObjectReference playerRef = Alias_owningPlayer.GetReference()
    ObjectReference rewardRef = Alias_RewardWeapon.GetReference()
    If playerRef && rewardRef
        playerRef.RemoveItem(rewardRef.GetBaseObject(), 1, False, Alias_Roper.GetReference())
    EndIf
EndFunction

Function Fragment_Stage_1244_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQ_004P_Crane_RoperResolutionIndex, 4.0)
    EndIf
EndFunction

Function Fragment_Stage_1245_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.RemoveItem(Caps001, 100, False)
        playerRef.SetValue(W05_MQ_004P_Crane_RoperResolutionIndex, 5.0)
    EndIf
EndFunction

Function Fragment_Stage_1250_Item_00()
    SetObjectiveCompleted(1230)
    If !IsStageDone(1300)
        SetStage(1300)
    EndIf
EndFunction

Function Fragment_Stage_1260_Item_00()
    SetObjectiveCompleted(1200)
    Actor roperRef = Alias_Roper.GetActorReference()
    If roperRef
        roperRef.EvaluatePackage()
    EndIf
    If !IsStageDone(1261)
        SetStage(1261)
    EndIf
EndFunction

Function Fragment_Stage_1261_Item_00()
    Actor roperRef = Alias_Roper.GetActorReference()
    If roperRef
        roperRef.EvaluatePackage()
    EndIf
    Int radicalIndex = 0
    While radicalIndex < Alias_Radicals.GetCount()
        Actor radicalRef = Alias_Radicals.GetAt(radicalIndex) as Actor
        If radicalRef
            radicalRef.EvaluatePackage()
        EndIf
        radicalIndex += 1
    EndWhile
EndFunction

Function Fragment_Stage_1265_Item_00()
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef
        playerRef.AddItem(Headwear_Radicals, 1, False)
        playerRef.SetValue(W05_MQ_004P_Crane_PlayerReceivedRadicalsGear, 1.0)
        playerRef.SetValue(W05_MQ_004P_Crane_PlayerJoinedRadicals, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    SetObjectiveCompleted(1200)
    SetObjectiveCompleted(1230)
    SetObjectiveDisplayed(1100)
    If W05_MQ_004P_Crane_1300_DuchessAttractScene && !W05_MQ_004P_Crane_1300_DuchessAttractScene.IsPlaying()
        W05_MQ_004P_Crane_1300_DuchessAttractScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_8999_Item_00()
    SetObjectiveCompleted(1100)
    SetObjectiveCompleted(1200)
    SetObjectiveCompleted(1230)
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(1100)
    SetObjectiveCompleted(1200)
    SetObjectiveCompleted(1230)
    ObjectReference playerRef = Alias_owningPlayer.GetReference()
    If W05_MQ_101P && !W05_MQ_101P.IsRunning() && !W05_MQ_101P.IsCompleted()
        W05_MQ_101P_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
    W05_WaywardSettlement_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
EndFunction
