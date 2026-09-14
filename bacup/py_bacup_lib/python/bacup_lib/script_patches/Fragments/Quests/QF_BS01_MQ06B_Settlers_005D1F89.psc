Event OnQuestInit()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == Game.GetPlayer()
        UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
    RegisterQuestActivations()
EndEvent

Event OnQuestShutdown()
    CancelTimer(9000)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
    UnregisterQuestActivations()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender != Game.GetPlayer()
        Return
    EndIf
    UnregisterForRemoteEvent(akSender, "OnPlayerLoadGame")
    RegisterForRemoteEvent(akSender, "OnPlayerLoadGame")
    RegisterQuestActivations()
    If IsStageDone(9000)
        TryCompleteSuccessorHandoff()
    EndIf
EndEvent

Function RegisterQuestActivations()
    UnregisterQuestActivations()
    If !IsStageDone(400) || IsStageDone(430) || IsStageDone(440) || IsStageDone(450)
        Return
    EndIf

    RefCollectionAlias bodies = GetAlias(55) as RefCollectionAlias
    RefCollectionAlias crates = GetAlias(56) as RefCollectionAlias
    ReferenceAlias explosionAlias = GetAlias(57) as ReferenceAlias
    ReferenceAlias bagAlias = GetAlias(82) as ReferenceAlias
    Int index = 0
    While bodies != None && !IsStageDone(410) && index < bodies.GetCount()
        ObjectReference bodyRef = bodies.GetAt(index)
        If bodyRef != None
            RegisterForRemoteEvent(bodyRef, "OnActivate")
        EndIf
        index += 1
    EndWhile
    index = 0
    While crates != None && !IsStageDone(411) && index < crates.GetCount()
        ObjectReference crateRef = crates.GetAt(index)
        If crateRef != None
            RegisterForRemoteEvent(crateRef, "OnActivate")
        EndIf
        index += 1
    EndWhile
    If explosionAlias != None && explosionAlias.GetReference() != None && !IsStageDone(412)
        RegisterForRemoteEvent(explosionAlias.GetReference(), "OnActivate")
    EndIf
    If bagAlias != None && bagAlias.GetReference() != None && !IsStageDone(430)
        RegisterForRemoteEvent(bagAlias.GetReference(), "OnActivate")
    EndIf
EndFunction

Function UnregisterQuestActivations()
    RefCollectionAlias bodies = GetAlias(55) as RefCollectionAlias
    RefCollectionAlias crates = GetAlias(56) as RefCollectionAlias
    ReferenceAlias explosionAlias = GetAlias(57) as ReferenceAlias
    ReferenceAlias bagAlias = GetAlias(82) as ReferenceAlias
    Int index = 0
    While bodies != None && index < bodies.GetCount()
        ObjectReference bodyRef = bodies.GetAt(index)
        If bodyRef != None
            UnregisterForRemoteEvent(bodyRef, "OnActivate")
        EndIf
        index += 1
    EndWhile
    index = 0
    While crates != None && index < crates.GetCount()
        ObjectReference crateRef = crates.GetAt(index)
        If crateRef != None
            UnregisterForRemoteEvent(crateRef, "OnActivate")
        EndIf
        index += 1
    EndWhile
    If explosionAlias != None && explosionAlias.GetReference() != None
        UnregisterForRemoteEvent(explosionAlias.GetReference(), "OnActivate")
    EndIf
    If bagAlias != None && bagAlias.GetReference() != None
        UnregisterForRemoteEvent(bagAlias.GetReference(), "OnActivate")
    EndIf
EndFunction

Bool Function TryHandleClueActivation(ObjectReference akSender)
    If !IsStageDone(400) || IsStageDone(430) || IsStageDone(440) || IsStageDone(450)
        Return False
    EndIf

    RefCollectionAlias bodies = GetAlias(55) as RefCollectionAlias
    Int index = 0
    While bodies != None && !IsStageDone(410) && index < bodies.GetCount()
        If bodies.GetAt(index) == akSender
            Message bodyMessage = Game.GetFormFromFile(0x005D99CA, "SeventySix.esm") as Message
            If bodyMessage != None
                bodyMessage.Show()
            EndIf
            UnregisterForRemoteEvent(akSender, "OnActivate")
            SetStage(410)
            Return True
        EndIf
        index += 1
    EndWhile

    RefCollectionAlias crates = GetAlias(56) as RefCollectionAlias
    index = 0
    While crates != None && !IsStageDone(411) && index < crates.GetCount()
        If crates.GetAt(index) == akSender
            Message crateMessage = Game.GetFormFromFile(0x005D99CC, "SeventySix.esm") as Message
            If crateMessage != None
                crateMessage.Show()
            EndIf
            UnregisterForRemoteEvent(akSender, "OnActivate")
            SetStage(411)
            Return True
        EndIf
        index += 1
    EndWhile

    ReferenceAlias explosionAlias = GetAlias(57) as ReferenceAlias
    If explosionAlias != None && explosionAlias.GetReference() == akSender && !IsStageDone(412)
        Message explosionMessage = Game.GetFormFromFile(0x005D99C6, "SeventySix.esm") as Message
        If explosionMessage != None
            explosionMessage.Show()
        EndIf
        UnregisterForRemoteEvent(akSender, "OnActivate")
        SetStage(412)
        Return True
    EndIf
    Return False
EndFunction

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    Actor playerRef = Alias_Player.GetActorReference()
    ReferenceAlias bagAlias = GetAlias(82) as ReferenceAlias
    If playerRef == None || playerRef != Game.GetPlayer() || akActionRef != playerRef
        Return
    EndIf
    If TryHandleClueActivation(akSender)
        Return
    EndIf
    If bagAlias == None || bagAlias.GetReference() != akSender || !IsStageDone(400) || IsStageDone(430) || IsStageDone(440) || IsStageDone(450)
        Return
    EndIf

    Message bagMessage = Game.GetFormFromFile(0x005DB4F3, "SeventySix.esm") as Message
    If bagMessage != None && bagMessage.Show() != 0
        Return
    EndIf
    If playerRef.GetItemCount(EvidenceBaseItem) > 0
        Message passMessage = Game.GetFormFromFile(0x005DB4F2, "SeventySix.esm") as Message
        If passMessage != None
            passMessage.Show()
        EndIf
        SetStage(430)
        UnregisterQuestActivations()
    Else
        Message failMessage = Game.GetFormFromFile(0x005DB4F4, "SeventySix.esm") as Message
        If failMessage != None
            failMessage.Show()
        EndIf
        If !IsStageDone(413)
            SetStage(413)
        EndIf
    EndIf
EndEvent

Function TryCompleteSuccessorHandoff()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    Bool accepted = False
    If BS01_MQ07_Over != None
        accepted = BS01_MQ07_Over.IsRunning() || BS01_MQ07_Over.IsCompleted()
        If !accepted && playerRef != None && BS01_MQ07_Over_StartKeyword != None
            accepted = BS01_MQ07_Over_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        EndIf
        accepted = accepted || BS01_MQ07_Over.IsRunning() || BS01_MQ07_Over.IsCompleted()
    EndIf
    If accepted
        CancelTimer(9000)
        Stop()
    Else
        CancelTimer(9000)
        StartTimer(5.0, 9000)
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 9000 && IsStageDone(9000)
        TryCompleteSuccessorHandoff()
    EndIf
EndEvent

Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Game.GetPlayer()
    If Alias_Player != None
        Alias_Player.ForceRefIfEmpty(playerRef)
        Actor aliasPlayer = Alias_Player.GetActorReference()
        If aliasPlayer != None
            playerRef = aliasPlayer
        EndIf
    EndIf
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
        playerRef.SetValue(AV_RahmaniOpinion, 0.0)
        playerRef.SetValue(AV_Foundation, 0.0)
        playerRef.SetValue(AV_MikeDead, 0.0)
        playerRef.SetValue(AV_Ready, 0.0)
        playerRef.SetValue(AV_ValuablesCurrent, 0.0)
    EndIf
    Global_ValuablesMax.SetValue(3.0)
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
    SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0201_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        playerRef.SetValue(AV_RahmaniOpinion, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0202_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        playerRef.SetValue(AV_RahmaniOpinion, 2.0)
    EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
    SetObjectiveCompleted(400)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0310_Item_00()
    Actor gloriaRef = Gloria.GetActorReference()
    Actor tadRef = Tad.GetActorReference()
    If gloriaRef != None
        gloriaRef.EvaluatePackage()
    EndIf
    If tadRef != None
        tadRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0325_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(35)
EndFunction

Function Fragment_Stage_0350_Item_00()
    SetObjectiveCompleted(35)
    SetObjectiveDisplayed(400, False)
    SetObjectiveDisplayed(50)
    If Marker_Tower != None
        Marker_Tower.AddToMap()
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    RegisterQuestActivations()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(55)
EndFunction

Function Fragment_Stage_0410_Item_00()
    If IsStageDone(411) && IsStageDone(412) && !IsStageDone(420)
        SetStage(420)
        SetObjectiveDisplayed(60)
    EndIf
EndFunction

Function Fragment_Stage_0411_Item_00()
    If IsStageDone(410) && IsStageDone(412) && !IsStageDone(420)
        SetStage(420)
        SetObjectiveDisplayed(60)
    EndIf
EndFunction

Function Fragment_Stage_0412_Item_00()
    If IsStageDone(410) && IsStageDone(411) && !IsStageDone(420)
        SetStage(420)
        SetObjectiveDisplayed(60)
    EndIf
EndFunction

Function Fragment_Stage_0413_Item_00()
    SetObjectiveDisplayed(67)
EndFunction

Function Fragment_Stage_0421_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    ObjectReference holotapeRef = HolotapeQOAlias.GetReference()
    ObjectReference evidenceRef = EvidenceAlias.GetReference()
    If playerRef != None
        If holotapeRef == None && playerRef.GetItemCount(HolotapeBaseItem) == 0
            holotapeRef = playerRef.PlaceAtMe(HolotapeBaseItem, 1, True, True, False)
            If holotapeRef != None
                HolotapeQOAlias.ForceRefTo(holotapeRef)
            EndIf
        EndIf
        If playerRef.GetItemCount(HolotapeBaseItem) == 0
            If holotapeRef != None
                playerRef.AddItem(holotapeRef, 1, True)
            Else
                playerRef.AddItem(HolotapeBaseItem, 1, True)
            EndIf
        EndIf
        If evidenceRef == None && playerRef.GetItemCount(EvidenceBaseItem) == 0
            evidenceRef = playerRef.PlaceAtMe(EvidenceBaseItem, 1, True, True, False)
            If evidenceRef != None
                EvidenceAlias.ForceRefTo(evidenceRef)
            EndIf
        EndIf
        If playerRef.GetItemCount(EvidenceBaseItem) == 0
            If evidenceRef != None
                playerRef.AddItem(evidenceRef, 1, True)
            Else
                playerRef.AddItem(EvidenceBaseItem, 1, True)
            EndIf
        EndIf
    EndIf
    ObjectReference dispenserRef = HolotapeDispenserAlias.GetReference()
    If dispenserRef != None
        dispenserRef.Disable()
    EndIf
EndFunction

Function Fragment_Stage_0422_Item_00()
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(65)
EndFunction

Function Fragment_Stage_0425_Item_00()
    SetObjectiveDisplayed(67)
EndFunction

Function Fragment_Stage_0430_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    ObjectReference passwordRef = PasswordAlias.GetReference()
    If playerRef != None
        If passwordRef == None && playerRef.GetItemCount(PasswordBaseItem) == 0
            passwordRef = playerRef.PlaceAtMe(PasswordBaseItem, 1, True, True, False)
            If passwordRef != None
                PasswordAlias.ForceRefTo(passwordRef)
            EndIf
        EndIf
        If playerRef.GetItemCount(PasswordBaseItem) == 0
            If passwordRef != None
                playerRef.AddItem(passwordRef, 1, True)
            Else
                playerRef.AddItem(PasswordBaseItem, 1, True)
            EndIf
        EndIf
    EndIf
    UnregisterQuestActivations()
    SetObjectiveCompleted(67)
EndFunction

Function Fragment_Stage_0440_Item_00()
    UnregisterQuestActivations()
    SetObjectiveCompleted(65)
EndFunction

Function Fragment_Stage_0450_Item_00()
    UnregisterQuestActivations()
    SetObjectiveCompleted(55)
    SetObjectiveDisplayed(67, False)
    SetObjectiveDisplayed(70)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveDisplayed(70)
EndFunction

Function Fragment_Stage_0625_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    ObjectReference lockedDoorRef = DoorLocked.GetReference()
    ObjectReference barredDoorRef = DoorBarred.GetReference()
    If playerRef != None
        playerRef.SetValue(AV_ValuablesCurrent, 0.0)
    EndIf
    Global_ValuablesMax.SetValue(3.0)
    If lockedDoorRef != None
        lockedDoorRef.Enable()
    EndIf
    If barredDoorRef != None
        barredDoorRef.Enable()
    EndIf
    SetObjectiveDisplayed(501)
EndFunction

Function Fragment_Stage_0650_Item_00()
    SetObjectiveCompleted(70)
    SetObjectiveDisplayed(80)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(80)
    SetObjectiveDisplayed(90)
EndFunction

Function Fragment_Stage_0710_Item_00()
    SetObjectiveDisplayed(500)
    If !IsStageDone(749)
        SetStage(749)
    EndIf
EndFunction

Function Fragment_Stage_0725_Item_00()
    SetObjectiveDisplayed(500)
    If !IsStageDone(749)
        SetStage(749)
    EndIf
EndFunction

Function Fragment_Stage_0726_Item_00()
    SetObjectiveDisplayed(500)
    If !IsStageDone(749)
        SetStage(749)
    EndIf
EndFunction

Function Fragment_Stage_0749_Item_00()
    ObjectReference crateRef = DiveSuit_Crate.GetReference()
    ObjectReference knifeRef = DiveSuit_Knife.GetReference()
    ObjectReference noteRef = DiveSuit_Note.GetReference()
    ObjectReference suitRef = DiveSuit_Suit.GetReference()
    If crateRef != None
        crateRef.Enable()
        If crateRef.GetItemCount(DiveSuit_Suit_BaseItem) == 0
            If suitRef == None
                suitRef = crateRef.PlaceAtMe(DiveSuit_Suit_BaseItem, 1, True, True, False)
                If suitRef != None
                    DiveSuit_Suit.ForceRefTo(suitRef)
                EndIf
            EndIf
            If suitRef != None
                crateRef.AddItem(suitRef, 1, True)
            Else
                crateRef.AddItem(DiveSuit_Suit_BaseItem, 1, True)
            EndIf
        EndIf
    EndIf
    If knifeRef != None
        knifeRef.Enable()
    EndIf
    If noteRef != None
        noteRef.Enable()
    EndIf
EndFunction

Function Fragment_Stage_0750_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    ObjectReference suitRef = DiveSuit_Suit.GetReference()
    If playerRef != None && playerRef.GetItemCount(DiveSuit_Suit_BaseItem) == 0
        If suitRef != None
            playerRef.AddItem(suitRef, 1, True)
        Else
            playerRef.AddItem(DiveSuit_Suit_BaseItem, 1, True)
        EndIf
    EndIf
    SetObjectiveCompleted(500)
EndFunction

Function Fragment_Stage_0775_Item_00()
    SetObjectiveDisplayed(95)
    If !IsStageDone(799)
        SetStage(799)
    EndIf
EndFunction

Function Fragment_Stage_0799_Item_00()
    ObjectReference keyCrateRef = KeyCrate.GetReference()
    ObjectReference keyRef = KeyAlias.GetReference()
    If keyCrateRef != None
        keyCrateRef.Enable()
        If keyCrateRef.GetItemCount(KeyBaseItem) == 0
            If keyRef == None
                keyRef = keyCrateRef.PlaceAtMe(KeyBaseItem, 1, True, True, False)
                If keyRef != None
                    KeyAlias.ForceRefTo(keyRef)
                EndIf
            EndIf
            If keyRef != None
                keyCrateRef.AddItem(keyRef, 1, True)
            Else
                keyCrateRef.AddItem(KeyBaseItem, 1, True)
            EndIf
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    ObjectReference keyRef = KeyAlias.GetReference()
    If playerRef != None && playerRef.GetItemCount(KeyBaseItem) == 0
        If keyRef != None
            playerRef.AddItem(keyRef, 1, True)
        Else
            playerRef.AddItem(KeyBaseItem, 1, True)
        EndIf
    EndIf
    SetObjectiveCompleted(95)
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0801_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    ObjectReference valuableRef = Valuables01Alias.GetReference()
    If playerRef != None
        If valuableRef == None && playerRef.GetItemCount(Valuables01BaseItem) == 0
            valuableRef = playerRef.PlaceAtMe(Valuables01BaseItem, 1, True, True, False)
            If valuableRef != None
                Valuables01Alias.ForceRefTo(valuableRef)
            EndIf
        EndIf
        If playerRef.GetItemCount(Valuables01BaseItem) == 0
            If valuableRef != None
                playerRef.AddItem(valuableRef, 1, True)
            Else
                playerRef.AddItem(Valuables01BaseItem, 1, True)
            EndIf
        EndIf
        playerRef.ModValue(AV_ValuablesCurrent, 1.0)
        If playerRef.GetValue(AV_ValuablesCurrent) >= Global_ValuablesMax.GetValue() && !IsStageDone(804)
            SetStage(804)
        EndIf
    EndIf
    ObjectReference dispenserRef = Valuables01Dispenser.GetReference()
    If dispenserRef != None
        dispenserRef.Disable()
    EndIf
EndFunction

Function Fragment_Stage_0802_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    ObjectReference valuableRef = Valuables02Alias.GetReference()
    If playerRef != None
        If valuableRef == None && playerRef.GetItemCount(Valuables02BaseItem) == 0
            valuableRef = playerRef.PlaceAtMe(Valuables02BaseItem, 1, True, True, False)
            If valuableRef != None
                Valuables02Alias.ForceRefTo(valuableRef)
            EndIf
        EndIf
        If playerRef.GetItemCount(Valuables02BaseItem) == 0
            If valuableRef != None
                playerRef.AddItem(valuableRef, 1, True)
            Else
                playerRef.AddItem(Valuables02BaseItem, 1, True)
            EndIf
        EndIf
        playerRef.ModValue(AV_ValuablesCurrent, 1.0)
        If playerRef.GetValue(AV_ValuablesCurrent) >= Global_ValuablesMax.GetValue() && !IsStageDone(804)
            SetStage(804)
        EndIf
    EndIf
    ObjectReference dispenserRef = Valuables02Dispenser.GetReference()
    If dispenserRef != None
        dispenserRef.Disable()
    EndIf
EndFunction

Function Fragment_Stage_0803_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    ObjectReference valuableRef = Valuables03Alias.GetReference()
    If playerRef != None
        If valuableRef == None && playerRef.GetItemCount(Valuables03BaseItem) == 0
            valuableRef = playerRef.PlaceAtMe(Valuables03BaseItem, 1, True, True, False)
            If valuableRef != None
                Valuables03Alias.ForceRefTo(valuableRef)
            EndIf
        EndIf
        If playerRef.GetItemCount(Valuables03BaseItem) == 0
            If valuableRef != None
                playerRef.AddItem(valuableRef, 1, True)
            Else
                playerRef.AddItem(Valuables03BaseItem, 1, True)
            EndIf
        EndIf
        playerRef.ModValue(AV_ValuablesCurrent, 1.0)
        If playerRef.GetValue(AV_ValuablesCurrent) >= Global_ValuablesMax.GetValue() && !IsStageDone(804)
            SetStage(804)
        EndIf
    EndIf
    ObjectReference dispenserRef = Valuables03Dispenser.GetReference()
    If dispenserRef != None
        dispenserRef.Disable()
    EndIf
EndFunction

Function Fragment_Stage_0804_Item_00()
    SetObjectiveCompleted(501)
EndFunction

Function Fragment_Stage_0826_Item_00()
    ObjectReference lockedDoorRef = DoorLocked.GetReference()
    If lockedDoorRef != None
        lockedDoorRef.Lock(False)
    EndIf
    SetObjectiveCompleted(100)
    SetObjectiveCompleted(90)
    SetObjectiveDisplayed(105)
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(105)
    SetObjectiveDisplayed(110)
EndFunction

Function Fragment_Stage_0910_Item_00()
    Actor mikeRef = MineMike.GetActorReference()
    If mikeRef != None
        mikeRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        playerRef.SetValue(AV_MikeDead, 2.0)
    EndIf
    SetObjectiveCompleted(110)
    If !IsStageDone(1075)
        SetStage(1075)
    EndIf
EndFunction

Function Fragment_Stage_1001_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        playerRef.SetValue(AV_MikeDead, 2.0)
    EndIf
    SetObjectiveCompleted(110)
    SetObjectiveDisplayed(120)
    If !IsStageDone(1030)
        SetStage(1030)
    EndIf
EndFunction

Function Fragment_Stage_1002_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        playerRef.SetValue(AV_MikeDead, 1.0)
    EndIf
    SetObjectiveCompleted(110)
    SetObjectiveDisplayed(120)
    If !IsStageDone(1030)
        SetStage(1030)
    EndIf
EndFunction

Function Fragment_Stage_1003_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    Actor mikeRef = MineMike.GetActorReference()
    ObjectReference weaponKeyRef = WepKeyAlias.GetReference()
    If playerRef != None
        playerRef.SetValue(AV_MikeDead, 1.0)
    EndIf
    If mikeRef != None && mikeRef.GetItemCount(WepKeyBaseItem) == 0
        If weaponKeyRef == None
            weaponKeyRef = mikeRef.PlaceAtMe(WepKeyBaseItem, 1, True, True, False)
            If weaponKeyRef != None
                WepKeyAlias.ForceRefTo(weaponKeyRef)
            EndIf
        EndIf
        If weaponKeyRef != None
            mikeRef.AddItem(weaponKeyRef, 1, True)
        Else
            mikeRef.AddItem(WepKeyBaseItem, 1, True)
        EndIf
    EndIf
    SetObjectiveCompleted(110)
    SetObjectiveCompleted(115)
    SetObjectiveDisplayed(120)
    If !IsStageDone(1030)
        SetStage(1030)
    EndIf
EndFunction

Function Fragment_Stage_1030_Item_00()
    ObjectReference barredDoorRef = DoorBarred.GetReference()
    ObjectReference cacheCrateRef = CacheCrate.GetReference()
    ObjectReference cacheRef = CacheAlias.GetReference()
    If barredDoorRef != None
        barredDoorRef.Disable()
    EndIf
    If cacheCrateRef != None
        cacheCrateRef.Enable()
        If cacheCrateRef.GetItemCount(CacheBaseItem) == 0
            If cacheRef == None
                cacheRef = cacheCrateRef.PlaceAtMe(CacheBaseItem, 1, True, True, False)
                If cacheRef != None
                    CacheAlias.ForceRefTo(cacheRef)
                EndIf
            EndIf
            If cacheRef != None
                cacheCrateRef.AddItem(cacheRef, 1, True)
            Else
                cacheCrateRef.AddItem(CacheBaseItem, 1, True)
            EndIf
        EndIf
    EndIf
    SetObjectiveDisplayed(120)
EndFunction

Function Fragment_Stage_1040_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    ObjectReference weaponKeyRef = WepKeyAlias.GetReference()
    If playerRef != None && playerRef.GetItemCount(WepKeyBaseItem) == 0
        If weaponKeyRef != None
            playerRef.AddItem(weaponKeyRef, 1, True)
        Else
            playerRef.AddItem(WepKeyBaseItem, 1, True)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1041_Item_00()
    SetObjectiveDisplayed(120)
EndFunction

Function Fragment_Stage_1050_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    ObjectReference cacheRef = CacheAlias.GetReference()
    If playerRef != None && playerRef.GetItemCount(CacheBaseItem) == 0
        If cacheRef != None
            playerRef.AddItem(cacheRef, 1, True)
        Else
            playerRef.AddItem(CacheBaseItem, 1, True)
        EndIf
    EndIf
    SetObjectiveCompleted(120)
    If !IsStageDone(1075)
        SetStage(1075)
    EndIf
EndFunction

Function Fragment_Stage_1075_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        playerRef.SetValue(AV_Ready, 1.0)
    EndIf
    SetObjectiveCompleted(90)
    SetObjectiveCompleted(110)
    SetObjectiveCompleted(115)
    SetObjectiveCompleted(120)
    SetObjectiveDisplayed(130)
    If !IsStageDone(9999)
        SetStage(9999)
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveCompleted(130)
    SetObjectiveDisplayed(140)
EndFunction

Function Fragment_Stage_1110_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    Actor gloriaRef = Gloria.GetActorReference()
    Actor tadRef = Tad.GetActorReference()
    If gloriaRef != None
        gloriaRef.EvaluatePackage()
    EndIf
    If tadRef != None
        tadRef.EvaluatePackage()
    EndIf
    If playerRef != None && playerRef.GetValue(AV_MikeDead) == 2.0 && scene_MikeTalk != None && !scene_MikeTalk.IsPlaying()
        scene_MikeTalk.Start()
    EndIf
EndFunction

Function Fragment_Stage_1150_Item_00()
    SetObjectiveCompleted(140)
    SetObjectiveDisplayed(150)
EndFunction

Function Fragment_Stage_1200_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        playerRef.SetValue(AV_Ready, 0.0)
    EndIf
    SetObjectiveCompleted(150)
    SetObjectiveDisplayed(160)
EndFunction

Function Fragment_Stage_1201_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        If playerRef.GetItemCount(Caps001) >= 1000
            playerRef.RemoveItem(Caps001, 1000, True)
        EndIf
        playerRef.SetValue(FoundationDecision, 6.0)
    EndIf
EndFunction

Function Fragment_Stage_1202_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        playerRef.SetValue(FoundationDecision, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_1203_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        playerRef.SetValue(FoundationDecision, 4.0)
    EndIf
EndFunction

Function Fragment_Stage_1204_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        playerRef.SetValue(FoundationDecision, 3.0)
    EndIf
EndFunction

Function Fragment_Stage_1205_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        playerRef.SetValue(FoundationDecision, 5.0)
    EndIf
EndFunction

Function Fragment_Stage_1206_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        playerRef.SetValue(FoundationDecision, 2.0)
    EndIf
EndFunction

Function Fragment_Stage_1207_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        playerRef.SetValue(FoundationDecision, 7.0)
    EndIf
EndFunction

Function Fragment_Stage_1211_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    Actor gloriaRef = Gloria.GetActorReference()
    Actor tadRef = Tad.GetActorReference()
    If playerRef != None
        playerRef.SetValue(AV_Foundation, 1.0)
    EndIf
    If gloriaRef != None
        gloriaRef.ChangeAnimFaceArchetype(FaceWorried)
        gloriaRef.ChangeAnimArchetype(AnimWorried)
    EndIf
    If tadRef != None
        tadRef.ChangeAnimFaceArchetype(FaceWorried)
        tadRef.ChangeAnimArchetype(AnimWorried)
    EndIf
EndFunction

Function Fragment_Stage_1212_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    Actor gloriaRef = Gloria.GetActorReference()
    Actor tadRef = Tad.GetActorReference()
    If playerRef != None
        playerRef.SetValue(AV_Foundation, 2.0)
    EndIf
    If gloriaRef != None
        gloriaRef.ChangeAnimFaceArchetype(FaceFriendly)
        gloriaRef.ChangeAnimArchetype(AnimFriendly)
    EndIf
    If tadRef != None
        tadRef.ChangeAnimFaceArchetype(FaceFriendly)
        tadRef.ChangeAnimArchetype(AnimFriendly)
    EndIf
EndFunction

Function Fragment_Stage_1213_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    Actor gloriaRef = Gloria.GetActorReference()
    Actor tadRef = Tad.GetActorReference()
    If playerRef != None
        playerRef.SetValue(AV_Foundation, 3.0)
    EndIf
    If gloriaRef != None
        gloriaRef.ChangeAnimFaceArchetype(FaceConfident)
        gloriaRef.ChangeAnimArchetype(AnimConfident)
    EndIf
    If tadRef != None
        tadRef.ChangeAnimFaceArchetype(FaceConfident)
        tadRef.ChangeAnimArchetype(AnimConfident)
    EndIf
EndFunction

Function Fragment_Stage_1250_Item_00()
    SetObjectiveCompleted(150)
    SetObjectiveDisplayed(160)
EndFunction

Function Fragment_Stage_1297_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && !IsStageDone(1298)
        If playerRef.GetItemCount(Caps001) >= 2500
            playerRef.RemoveItem(Caps001, 2500, True)
        EndIf
        playerRef.SetValue(FoundationDecision, 6.0)
        SetStage(1298)
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    SetObjectiveCompleted(160)
EndFunction

Function Fragment_Stage_1400_Item_00()
    SetObjectiveCompleted(160)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteQuest()
    TryCompleteSuccessorHandoff()
EndFunction

Function Fragment_Stage_9999_Item_00()
    MineMike.Clear()
EndFunction
