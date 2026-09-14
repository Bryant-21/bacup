Function Fragment_Stage_0004_Item_00()
    ObjectReference entranceDoor = Alias_Vault79EntranceDoor.GetReference()
    If entranceDoor
        entranceDoor.Enable()
    EndIf
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0005_Item_00()
    Actor johnnyRef = Alias_Johnny.GetActorReference()
    Actor gailRef = Alias_Gail.GetActorReference()
    Actor raRaRef = Alias_RaRa.GetActorReference()
    If johnnyRef
        johnnyRef.EvaluatePackage()
    EndIf
    If gailRef
        gailRef.EvaluatePackage()
    EndIf
    If raRaRef
        raRaRef.EvaluatePackage()
    EndIf
    If !IsStageDone(4)
        SetStage(4)
    EndIf
EndFunction

Function Fragment_Stage_0006_Item_00()
    Actor jenRef = Alias_Jen.GetActorReference()
    Actor pennyRef = Alias_Penny.GetActorReference()
    Actor radcliffRef = Alias_Radcliff.GetActorReference()
    If jenRef
        jenRef.EvaluatePackage()
    EndIf
    If pennyRef
        pennyRef.EvaluatePackage()
    EndIf
    If radcliffRef
        radcliffRef.EvaluatePackage()
    EndIf
    If !IsStageDone(4)
        SetStage(4)
    EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
    If !IsStageDone(21)
        SetStage(21)
    EndIf
EndFunction

Function Fragment_Stage_0021_Item_00()
    Actor chaseRef = Alias_ChaseTerrier.GetActorReference()
    If chaseRef
        chaseRef.RemoveFromFaction(W05_SecretServiceEnemyFaction)
        chaseRef.StopCombatAlarm()
        chaseRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0022_Item_00()
    Actor chaseRef = Alias_ChaseTerrier.GetActorReference()
    Actor playerRef = Alias_Player.GetActorReference()
    If chaseRef && playerRef
        chaseRef.AddToFaction(W05_SecretServiceEnemyFaction)
        chaseRef.StartCombat(playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
    If W05_MQA_206P_SeeChase && !W05_MQA_206P_SeeChase.IsPlaying()
        W05_MQA_206P_SeeChase.Start()
    EndIf
EndFunction

Function Fragment_Stage_0033_Item_00()
    If W05_MQA_206P_Ghoul && !W05_MQA_206P_Ghoul.IsPlaying()
        W05_MQA_206P_Ghoul.Start()
    EndIf
EndFunction

Function Fragment_Stage_0035_Item_00()
    SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0050_Item_00()
    If W05_MQA_206P_GoldRoom && !W05_MQA_206P_GoldRoom.IsPlaying()
        W05_MQA_206P_GoldRoom.Start()
    EndIf
EndFunction

Function Fragment_Stage_0060_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    Actor jenRef = Alias_Jen.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQA_206P_JenGoneAV, 1.0)
    EndIf
    If jenRef
        jenRef.AddSpell(W05_MQS_205P_JenStealthSpell)
        jenRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0070_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    Actor jenRef = Alias_Jen.GetActorReference()
    If playerRef && playerRef.GetItemCount(W05_MQA_206P_JensNote) == 0
        playerRef.AddItem(W05_MQA_206P_JensNote, 1, True)
    EndIf
    If jenRef
        jenRef.RemoveSpell(W05_MQS_205P_JenStealthSpell)
        jenRef.Disable()
    EndIf
EndFunction

Function Fragment_Stage_0075_Item_00()
    Actor megRef = Alias_MegOperations.GetActorReference()
    Actor paigeRef = Alias_PaigeOperations.GetActorReference()
    If megRef
        megRef.Enable()
        megRef.EvaluatePackage()
    EndIf
    If paigeRef
        paigeRef.Enable()
        paigeRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0080_Item_00()
    Actor johnnyRef = Alias_JohnnyOperations.GetActorReference()
    Actor jenRef = Alias_JenOperations.GetActorReference()
    Actor radcliffRef = Alias_RadcliffOperations.GetActorReference()
    Actor reginaldRef = Alias_ReginaldStoneOperations.GetActorReference()
    Actor megRef = Alias_MegOperations.GetActorReference()
    Actor paigeRef = Alias_PaigeOperations.GetActorReference()
    If johnnyRef
        johnnyRef.Enable()
        johnnyRef.EvaluatePackage()
    EndIf
    If jenRef
        jenRef.Enable()
        jenRef.EvaluatePackage()
    EndIf
    If radcliffRef
        radcliffRef.Enable()
        radcliffRef.EvaluatePackage()
    EndIf
    If reginaldRef
        reginaldRef.Enable()
        reginaldRef.EvaluatePackage()
    EndIf
    If megRef
        megRef.Enable()
        megRef.EvaluatePackage()
    EndIf
    If paigeRef
        paigeRef.Enable()
        paigeRef.EvaluatePackage()
    EndIf
    SetObjectiveCompleted(600)
    SetObjectiveDisplayed(700)
    If !IsStageDone(75)
        SetStage(75)
    EndIf
EndFunction

Function Fragment_Stage_0081_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQA_206P_GoldChoiceCheckpointed, 1.0)
        If W05_MQR_205P.IsStageDone(9000)
            playerRef.SetValue(W05_GoldChoice_Crater, 1.0)
            playerRef.ModValue(Reputation_AV_Crater, Rep_Mod_Subtract_Large.GetValue())
            SetObjectiveDisplayed(801)
            SetObjectiveDisplayed(812, True, False)
        Else
            playerRef.SetValue(W05_GoldChoice_Foundation, 1.0)
            playerRef.ModValue(Reputation_AV_Foundation, Rep_Mod_Subtract_Large.GetValue())
            SetObjectiveDisplayed(800)
            SetObjectiveDisplayed(810, True, False)
            SetObjectiveDisplayed(811, True, False)
        EndIf
        SetObjectiveCompleted(700)
    EndIf
EndFunction

Function Fragment_Stage_0082_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQA_206P_GoldChoiceCheckpointed, 1.0)
        Int goldToGive = GoldToRemove
        Int availableGold = playerRef.GetItemCount(Gold_Bullion)
        If goldToGive > availableGold
            goldToGive = availableGold
        EndIf
        If goldToGive > 0
            playerRef.RemoveItem(Gold_Bullion, goldToGive, True)
        EndIf
        If W05_MQR_205P.IsStageDone(9000)
            playerRef.SetValue(W05_GoldChoice_Crater, 2.0)
            playerRef.ModValue(Reputation_AV_Crater, Rep_Mod_Add_Huge.GetValue())
            SetObjectiveDisplayed(801)
            SetObjectiveDisplayed(812, True, False)
        Else
            playerRef.SetValue(W05_GoldChoice_Foundation, 2.0)
            playerRef.ModValue(Reputation_AV_Foundation, Rep_Mod_Add_Huge.GetValue())
            SetObjectiveDisplayed(800)
            SetObjectiveDisplayed(810, True, False)
            SetObjectiveDisplayed(811, True, False)
        EndIf
        SetObjectiveCompleted(700)
    EndIf
EndFunction

Function Fragment_Stage_0083_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQA_206P_GoldChoiceCheckpointed, 1.0)
        Int goldToGive = GoldToRemove
        Int availableGold = playerRef.GetItemCount(Gold_Bullion)
        If goldToGive > availableGold
            goldToGive = availableGold
        EndIf
        If goldToGive > 0
            playerRef.RemoveItem(Gold_Bullion, goldToGive, True)
        EndIf
        playerRef.SetValue(W05_GoldChoice_Crater, 3.0)
        playerRef.SetValue(W05_GoldChoice_Foundation, 3.0)
        playerRef.ModValue(Reputation_AV_Crater, Rep_Mod_Add_Medium.GetValue())
        playerRef.ModValue(Reputation_AV_Foundation, Rep_Mod_Add_Medium.GetValue())
        SetObjectiveCompleted(700)
        SetObjectiveDisplayed(800)
        SetObjectiveDisplayed(801)
        SetObjectiveDisplayed(810, True, False)
        SetObjectiveDisplayed(811, True, False)
        SetObjectiveDisplayed(812, True, False)
    EndIf
EndFunction

Function Fragment_Stage_0085_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef && GoldBar != Gold_Bullion
        Int rawGold = playerRef.GetItemCount(Gold_Bullion)
        If rawGold > 0
            playerRef.RemoveItem(Gold_Bullion, rawGold, True)
            playerRef.AddItem(GoldBar, rawGold, True)
        EndIf
    EndIf
    ; FO76 account bullion deposits and daily-cap bookkeeping have no FO4 service.
    ; When both properties bind the same item, physical gold is retained locally.
EndFunction

Function Fragment_Stage_0090_Item_00()
    SetObjectiveCompleted(801)
    Actor megRef = Alias_MegOperations.GetActorReference()
    If megRef
        megRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0091_Item_00()
    SetObjectiveCompleted(812)
    Actor johnnyRef = Alias_JohnnyOperations.GetActorReference()
    If johnnyRef
        johnnyRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0092_Item_00()
    SetObjectiveCompleted(800)
    Actor paigeRef = Alias_PaigeOperations.GetActorReference()
    If paigeRef
        paigeRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0093_Item_00()
    SetObjectiveCompleted(810)
    Actor jenRef = Alias_JenOperations.GetActorReference()
    If jenRef
        jenRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0094_Item_00()
    SetObjectiveCompleted(811)
    Actor radcliffRef = Alias_RadcliffOperations.GetActorReference()
    If radcliffRef
        radcliffRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0095_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQR_JohnnyDeathReasonValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0096_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQR_JohnnyDeathReasonValue, 2.0)
    EndIf
EndFunction

Function Fragment_Stage_0097_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQR_JohnnyDeathReasonValue, 3.0)
    EndIf
EndFunction

Function Fragment_Stage_0098_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQR_JohnnyDeathReasonValue, 4.0)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    If W05_MQR_205P.IsStageDone(9000)
        SetStage(7)
    ElseIf W05_MQS_205P.IsStageDone(9000)
        SetStage(8)
    EndIf
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0105_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(105)
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveCompleted(105)
    If W05_MQA_206P_Greet && !W05_MQA_206P_Greet.IsPlaying()
        W05_MQA_206P_Greet.Start()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveCompleted(105)
    SetObjectiveDisplayed(200)
    Default2StateActivator louDoor = Alias_LouDoor.GetReference() as Default2StateActivator
    If louDoor
        ; FO76 SetActivatorOpen -> FO4 Default2StateActivator.SetOpen.
        louDoor.SetOpen(True)
    EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
    SetObjectiveCompleted(200)
    SetObjectiveDisplayed(250)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(250)
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(300)
    SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0450_Item_00()
    SetObjectiveCompleted(400)
    SetObjectiveDisplayed(440)
EndFunction

Function Fragment_Stage_0460_Item_00()
    SetObjectiveCompleted(440)
    SetObjectiveDisplayed(450)
    ObjectReference goldRoomDoor = Alias_GoldRoomDoor.GetReference()
    ObjectReference goldChest = Alias_GoldChest.GetReference()
    If goldRoomDoor
        goldRoomDoor.Enable()
    EndIf
    If goldChest
        goldChest.Enable()
    EndIf
    If Alias_GoldActivator01.GetReference()
        Alias_GoldActivator01.GetReference().Enable()
    EndIf
    If Alias_GoldActivator02.GetReference()
        Alias_GoldActivator02.GetReference().Enable()
    EndIf
    If Alias_GoldActivator03.GetReference()
        Alias_GoldActivator03.GetReference().Enable()
    EndIf
    If Alias_GoldActivator04.GetReference()
        Alias_GoldActivator04.GetReference().Enable()
    EndIf
    If Alias_GoldActivator05.GetReference()
        Alias_GoldActivator05.GetReference().Enable()
    EndIf
    If Alias_GoldActivator06.GetReference()
        Alias_GoldActivator06.GetReference().Enable()
    EndIf
    If Alias_GoldActivator07.GetReference()
        Alias_GoldActivator07.GetReference().Enable()
    EndIf
    If Alias_GoldActivator08.GetReference()
        Alias_GoldActivator08.GetReference().Enable()
    EndIf
    If Alias_GoldActivator09.GetReference()
        Alias_GoldActivator09.GetReference().Enable()
    EndIf
EndFunction

Function Fragment_Stage_0471_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    ObjectReference goldRef = Alias_GoldActivator01.GetReference()
    If playerRef
        playerRef.AddItem(Gold_Bullion, 100, True)
        playerRef.ModValue(W05_MQA_206P_CollectedBullion, 100.0)
    EndIf
    If goldRef
        goldRef.Disable()
    EndIf
EndFunction

Function Fragment_Stage_0472_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    ObjectReference goldRef = Alias_GoldActivator02.GetReference()
    If playerRef
        playerRef.AddItem(Gold_Bullion, 100, True)
        playerRef.ModValue(W05_MQA_206P_CollectedBullion, 100.0)
    EndIf
    If goldRef
        goldRef.Disable()
    EndIf
EndFunction

Function Fragment_Stage_0473_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    ObjectReference goldRef = Alias_GoldActivator03.GetReference()
    If playerRef
        playerRef.AddItem(Gold_Bullion, 100, True)
        playerRef.ModValue(W05_MQA_206P_CollectedBullion, 100.0)
    EndIf
    If goldRef
        goldRef.Disable()
    EndIf
EndFunction

Function Fragment_Stage_0474_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    ObjectReference goldRef = Alias_GoldActivator04.GetReference()
    If playerRef
        playerRef.AddItem(Gold_Bullion, 100, True)
        playerRef.ModValue(W05_MQA_206P_CollectedBullion, 100.0)
    EndIf
    If goldRef
        goldRef.Disable()
    EndIf
EndFunction

Function Fragment_Stage_0475_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    ObjectReference goldRef = Alias_GoldActivator05.GetReference()
    If playerRef
        playerRef.AddItem(Gold_Bullion, 100, True)
        playerRef.ModValue(W05_MQA_206P_CollectedBullion, 100.0)
    EndIf
    If goldRef
        goldRef.Disable()
    EndIf
EndFunction

Function Fragment_Stage_0476_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    ObjectReference goldRef = Alias_GoldActivator06.GetReference()
    If playerRef
        playerRef.AddItem(Gold_Bullion, 100, True)
        playerRef.ModValue(W05_MQA_206P_CollectedBullion, 100.0)
    EndIf
    If goldRef
        goldRef.Disable()
    EndIf
EndFunction

Function Fragment_Stage_0477_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    ObjectReference goldRef = Alias_GoldActivator07.GetReference()
    If playerRef
        playerRef.AddItem(Gold_Bullion, 100, True)
        playerRef.ModValue(W05_MQA_206P_CollectedBullion, 100.0)
    EndIf
    If goldRef
        goldRef.Disable()
    EndIf
EndFunction

Function Fragment_Stage_0478_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    ObjectReference goldRef = Alias_GoldActivator08.GetReference()
    If playerRef
        playerRef.AddItem(Gold_Bullion, 100, True)
        playerRef.ModValue(W05_MQA_206P_CollectedBullion, 100.0)
    EndIf
    If goldRef
        goldRef.Disable()
    EndIf
EndFunction

Function Fragment_Stage_0479_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    ObjectReference goldRef = Alias_GoldActivator09.GetReference()
    If playerRef
        playerRef.AddItem(Gold_Bullion, 100, True)
        playerRef.ModValue(W05_MQA_206P_CollectedBullion, 100.0)
    EndIf
    If goldRef
        goldRef.Disable()
    EndIf
EndFunction

Function Fragment_Stage_0481_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    ObjectReference goldChest = Alias_GoldChest.GetReference()
    If playerRef
        playerRef.AddItem(Gold_Bullion, 100, True)
        playerRef.ModValue(W05_MQA_206P_CollectedBullion, 100.0)
        If !IsStageDone(500) && playerRef.GetItemCount(Gold_Bullion) >= GoldToRemove
            SetStage(500)
        EndIf
    EndIf
    If goldChest
        goldChest.Disable()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(450)
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_0525_Item_00()
    SetObjectiveCompleted(500)
    SetObjectiveDisplayed(525)
EndFunction

Function Fragment_Stage_0550_Item_00()
    SetObjectiveDisplayed(550)
EndFunction

Function Fragment_Stage_0575_Item_00()
    SetObjectiveDisplayed(575)
EndFunction

Function Fragment_Stage_0585_Item_00()
    SetObjectiveCompleted(525)
    SetObjectiveDisplayed(550)
    If W05_MQA_206P_LiveOrDie && !W05_MQA_206P_LiveOrDie.IsPlaying()
        W05_MQA_206P_LiveOrDie.Start()
    EndIf
EndFunction

Function Fragment_Stage_0590_Item_00()
    SetObjectiveCompleted(550)
    SetObjectiveDisplayed(575)
    If !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(575)
    SetObjectiveDisplayed(600)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(600)
    SetObjectiveDisplayed(700)
    If W05_MQA_206P_OperationsScene && !W05_MQA_206P_OperationsScene.IsPlaying()
        W05_MQA_206P_OperationsScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(700)
    SetObjectiveCompleted(800)
    SetObjectiveCompleted(801)
    SetObjectiveCompleted(810)
    SetObjectiveCompleted(811)
    SetObjectiveCompleted(812)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_5000_Item_00()
    If W05_MQA_206P_Raiders_Leaving && !W05_MQA_206P_Raiders_Leaving.IsPlaying()
        W05_MQA_206P_Raiders_Leaving.Start()
    EndIf
EndFunction

Function Fragment_Stage_5010_Item_00()
    Actor gailRef = Alias_Gail.GetActorReference()
    If gailRef
        gailRef.EvaluatePackage()
    EndIf
    Actor raRaRef = Alias_RaRa.GetActorReference()
    If raRaRef
        raRaRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_5011_Item_00()
    If IsStageDone(5012) && !IsStageDone(5013)
        SetStage(5013)
    EndIf
EndFunction

Function Fragment_Stage_5012_Item_00()
    If IsStageDone(5011) && !IsStageDone(5013)
        SetStage(5013)
    EndIf
EndFunction

Function Fragment_Stage_5013_Item_00()
    Actor gailRef = Alias_Gail.GetActorReference()
    If gailRef
        gailRef.Disable()
    EndIf
    Actor raRaRef = Alias_RaRa.GetActorReference()
    If raRaRef
        raRaRef.Disable()
    EndIf
EndFunction

Function Fragment_Stage_5050_Item_00()
    If W05_MQA_206P_Johnny_001_Gold && !W05_MQA_206P_Johnny_001_Gold.IsPlaying()
        W05_MQA_206P_Johnny_001_Gold.Start()
    EndIf
EndFunction

Function Fragment_Stage_5200_Item_00()
    SetObjectiveDisplayed(5200)
EndFunction

Function Fragment_Stage_5205_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        Int johnnyShare = GoldToRemoveHalfToJohnny
        Int availableGold = playerRef.GetItemCount(Gold_Bullion)
        If johnnyShare > availableGold
            johnnyShare = availableGold
        EndIf
        If johnnyShare > 0
            playerRef.RemoveItem(Gold_Bullion, johnnyShare, True)
        EndIf
        playerRef.SetValue(W05_MQR_JohnnyCutValue, 2.0)
    EndIf
EndFunction

Function Fragment_Stage_5210_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        Int johnnyShare = GoldToRemoveMostToJohnny
        Int availableGold = playerRef.GetItemCount(Gold_Bullion)
        If johnnyShare > availableGold
            johnnyShare = availableGold
        EndIf
        If johnnyShare > 0
            playerRef.RemoveItem(Gold_Bullion, johnnyShare, True)
        EndIf
        playerRef.SetValue(W05_MQR_JohnnyCutValue, 3.0)
    EndIf
EndFunction

Function Fragment_Stage_5220_Item_00()
    SetObjectiveDisplayed(5220)
EndFunction

Function Fragment_Stage_5230_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    Actor johnnyRef = Alias_Johnny.GetActorReference()
    If playerRef
        playerRef.ModValue(W05_MQR_LouRelationshipValue, 1.0)
    EndIf
    If johnnyRef
        johnnyRef.StopCombatAlarm()
    EndIf
EndFunction

Function Fragment_Stage_5240_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    Actor johnnyRef = Alias_Johnny.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQA_206P_MustDealWithJohnnyAV, 0.0)
        playerRef.SetValue(W05_MQA_206P_JohnnyHostileAV, 0.0)
    EndIf
    If johnnyRef
        johnnyRef.RemoveFromFaction(W05_SecretServiceEnemyFaction)
        johnnyRef.StopCombatAlarm()
        johnnyRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_5250_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        If GoldToRemoveExtraJohnnyShare > 0
            playerRef.AddItem(Gold_Bullion, GoldToRemoveExtraJohnnyShare, True)
        EndIf
        playerRef.SetValue(W05_MQR_JohnnyCutValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_5275_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    Actor johnnyRef = Alias_Johnny.GetActorReference()
    If playerRef
        playerRef.SetValue(W05_MQA_206P_MustDealWithJohnnyAV, 1.0)
        playerRef.SetValue(W05_MQA_206P_JohnnyHostileAV, 1.0)
    EndIf
    If johnnyRef && playerRef
        johnnyRef.AddToFaction(W05_SecretServiceEnemyFaction)
        johnnyRef.StartCombat(playerRef)
    EndIf
EndFunction

Function Fragment_Stage_5300_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        Int reclaimedGold = W05_MQR_Gold_JohnnyDiesCut.GetValue() as Int
        If reclaimedGold > 0
            playerRef.AddItem(Gold_Bullion, reclaimedGold, True)
        EndIf
        playerRef.SetValue(W05_MQR_JohnnyDeadValue, 1.0)
        playerRef.SetValue(W05_MQA_206P_MustDealWithJohnnyAV, 0.0)
    EndIf
EndFunction

Function Fragment_Stage_5400_Item_00()
    Actor johnnyRef = Alias_Johnny.GetActorReference()
    If johnnyRef
        johnnyRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_5500_Item_00()
    Actor johnnyRef = Alias_Johnny.GetActorReference()
    ObjectReference operationsMarker = Alias_Vault79OperationsTeleportMarker.GetReference()
    If johnnyRef && operationsMarker
        johnnyRef.MoveTo(operationsMarker)
        johnnyRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    ObjectReference mapMarker = Alias_Vault79MapMarker.GetReference()
    If playerRef && playerRef.GetItemCount(W05_Vault79Operations_KeyCard) == 0
        playerRef.AddItem(W05_Vault79Operations_KeyCard, 1, True)
    EndIf
    If playerRef
        playerRef.SetValue(W05_MQS_206P_Checkpoint, 1.0)
    EndIf
    If mapMarker
        mapMarker.Enable()
    EndIf
    SetObjectiveCompleted(800)
    ; The stage's native CompleteQuest() flag performs quest completion.
    ; The native quest reward supplies local rewards. SCORE, achievements, account
    ; reputation replication, and daily bullion caps remain server-only.
    Quest louQuest = Game.GetFormFromFile(0x005588EF, "SeventySix.esm") as Quest
    If playerRef && louQuest && W05_MQR_205P && W05_MQR_205P.IsStageDone(9000) && W05_MQR_205P_A_QuestStart_Keyword && !louQuest.IsRunning() && !louQuest.IsCompleted()
        W05_MQR_205P_A_QuestStart_Keyword.SendStoryEventAndWait(None, playerRef, playerRef)
        If !louQuest.IsRunning() && !louQuest.IsCompleted()
            Debug.Trace("[B21 W05_MQR_205P_A] Story Manager selector 00559346 did not start quest 005588EF", 2)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_9999_Item_00()
    Stop()
EndFunction
