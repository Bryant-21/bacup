Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQS_204P_Started, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0015_Item_00()
    ObjectReference vaultMarker = Alias_EnableMarkerVault79.GetReference()
    If vaultMarker != None
        vaultMarker.EnableNoWait()
    EndIf
    If W05_MQS_204P_ActorEnableMarker != None
        W05_MQS_204P_ActorEnableMarker.EnableNoWait()
    EndIf
    If !IsStageDone(90)
        SetStage(90)
    EndIf
EndFunction

Function Fragment_Stage_0090_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(15)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(15)
    Actor paigeRef = Alias_Paige.GetActorReference()
    If paigeRef != None
        paigeRef.EvaluatePackage()
    EndIf
    If W05_MQS_204P_PaigeReactToHijackScene != None && !W05_MQS_204P_PaigeReactToHijackScene.IsPlaying()
        W05_MQS_204P_PaigeReactToHijackScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(15)
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0210_Item_00()
    If !IsStageDone(250)
        SetStage(250)
    EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && playerRef.GetItemCount(W05_MQS_204P_TheMazeID) < 1
        playerRef.AddItem(W05_MQS_204P_TheMazeID, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_0275_Item_00()
    If !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)
    If W05_MQS_204P_HijackWelcomeScene != None
        W05_MQS_204P_HijackWelcomeScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0360_Item_00()
    If !IsStageDone(370)
        SetStage(370)
    EndIf
EndFunction

Function Fragment_Stage_0370_Item_00()
    If W05_MQS_204P_RaidersDepart != None && !W05_MQS_204P_RaidersDepart.IsPlaying()
        W05_MQS_204P_RaidersDepart.Start()
    EndIf
    If Alias_HijackCrewAll != None
        Alias_HijackCrewAll.EvaluateAll()
    EndIf
    If !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0375_Item_00()
    ; FO76 awards Crater reputation here. FO4 has no account-backed reputation service.
    If !IsStageDone(370)
        SetStage(370)
    EndIf
EndFunction

Function Fragment_Stage_0380_Item_00()
    SetObjectiveDisplayed(35)
    Actor playerRef = Game.GetPlayer()
    If Alias_HijackCrewAll != None && playerRef != None
        If PlayerEnemyFaction != None
            Alias_HijackCrewAll.AddToFaction(PlayerEnemyFaction)
        EndIf
        Alias_HijackCrewAll.StartCombatAll(playerRef)
    EndIf
    If W05_MQS_204P_PennyReactStarCombat != None
        W05_MQS_204P_PennyReactStarCombat.Start()
    EndIf
EndFunction

Function Fragment_Stage_0390_Item_00()
    SetObjectiveCompleted(35)
    If W05_MQS_204P_PennyEndCombatScene != None
        W05_MQS_204P_PennyEndCombatScene.Start()
    EndIf
    If !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveCompleted(35)
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0525_Item_00()
    If W05_MQS_204P_PennyUseDoorTerminalScene != None
        W05_MQS_204P_PennyUseDoorTerminalScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0550_Item_00()
    ObjectReference doorRef = Alias_MazeDoor01.GetReference()
    If doorRef != None
        doorRef.SetOpen(True)
    EndIf
    doorRef = Alias_MazeDoor02.GetReference()
    If doorRef != None
        doorRef.SetOpen(True)
    EndIf
    doorRef = Alias_MazeDoor03.GetReference()
    If doorRef != None
        doorRef.SetOpen(True)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(60)
    If W05_MQS_204P_005_TheCaveScene != None
        W05_MQS_204P_005_TheCaveScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0610_Item_00()
    If !IsStageDone(615)
        SetStage(615)
    EndIf
EndFunction

Function Fragment_Stage_0615_Item_00()
    ObjectReference wallRef = Alias_FakeBarrier01.GetReference()
    Actor playerRef = Game.GetPlayer()
    If wallRef != None && playerRef != None
        wallRef.Activate(playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0620_Item_00()
    If !IsStageDone(625)
        SetStage(625)
    EndIf
EndFunction

Function Fragment_Stage_0625_Item_00()
    ObjectReference wallRef = Alias_FakeBarrier02.GetReference()
    Actor playerRef = Game.GetPlayer()
    If wallRef != None && playerRef != None
        wallRef.Activate(playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0630_Item_00()
    If !IsStageDone(635)
        SetStage(635)
    EndIf
EndFunction

Function Fragment_Stage_0635_Item_00()
    ObjectReference wallRef = Alias_FakeBarrier03.GetReference()
    Actor playerRef = Game.GetPlayer()
    If wallRef != None && playerRef != None
        wallRef.Activate(playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0640_Item_00()
    If !IsStageDone(645)
        SetStage(645)
    EndIf
EndFunction

Function Fragment_Stage_0645_Item_00()
    ObjectReference wallRef = Alias_FakeBarrier04.GetReference()
    Actor playerRef = Game.GetPlayer()
    If wallRef != None && playerRef != None
        wallRef.Activate(playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0650_Item_00()
    If !IsStageDone(655)
        SetStage(655)
    EndIf
EndFunction

Function Fragment_Stage_0655_Item_00()
    ObjectReference wallRef = Alias_FakeBarrier05.GetReference()
    Actor playerRef = Game.GetPlayer()
    If wallRef != None && playerRef != None
        wallRef.Activate(playerRef)
    EndIf
    If !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(70)
    If W05_MQS_204P_007_TestDoneScene != None
        W05_MQS_204P_007_TestDoneScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(70)
    SetObjectiveDisplayed(80)
    ObjectReference moduleContainer = Alias_ModuleContainer.GetReference()
    If moduleContainer != None && moduleContainer.GetItemCount(W05_MQS_204P_IntelligenceModule) < 1
        moduleContainer.AddItem(W05_MQS_204P_IntelligenceModule, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(80)
    SetObjectiveDisplayed(90)
    If W05_MQS_204P_009_IntelligenceModuleScene != None
        W05_MQS_204P_009_IntelligenceModuleScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(90)
    SetObjectiveDisplayed(100)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.RemoveItem(W05_MQS_204P_IntelligenceModule, 1, True)
    EndIf
    If W05_MQS_204P_010_RepairDoneScene != None
        W05_MQS_204P_010_RepairDoneScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveCompleted(90)
    SetObjectiveDisplayed(100)
    If W05_MQS_204P_MotherlodeRepairedScene != None && !W05_MQS_204P_MotherlodeRepairedScene.IsPlaying()
        W05_MQS_204P_MotherlodeRepairedScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(100)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        playerRef.SetValue(W05_MQ_204P_FactionChosen, 2.0)
    EndIf
    If W05_MQS_205P_QuestStartKeyword != None
        W05_MQS_205P_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction

Function Fragment_Stage_10000_Item_00()
    If W05_MQS_204P_HijackWelcomeScene != None
        W05_MQS_204P_HijackWelcomeScene.Stop()
    EndIf
    If W05_MQS_204P_PennyReactStarCombat != None
        W05_MQS_204P_PennyReactStarCombat.Stop()
    EndIf
    If W05_MQS_204P_PennyEndCombatScene != None
        W05_MQS_204P_PennyEndCombatScene.Stop()
    EndIf
    If W05_MQS_204P_RaidersDepart != None
        W05_MQS_204P_RaidersDepart.Stop()
    EndIf
    If W05_MQS_204P_ActorEnableMarker != None
        W05_MQS_204P_ActorEnableMarker.DisableNoWait()
    EndIf
    ObjectReference vaultMarker = Alias_EnableMarkerVault79.GetReference()
    If vaultMarker != None
        vaultMarker.DisableNoWait()
    EndIf
    ObjectReference navigationMarker = Alias_EnableMarkerNavigationTest.GetReference()
    If navigationMarker != None
        navigationMarker.DisableNoWait()
    EndIf
EndFunction
