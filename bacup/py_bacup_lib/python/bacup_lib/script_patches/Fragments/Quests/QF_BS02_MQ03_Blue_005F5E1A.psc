Function Fragment_Stage_0010_Item_00()
    If !IsStageDone(98)
        SetStage(98)
    Else
        SetObjectiveDisplayed(10)
    EndIf
EndFunction

Function Fragment_Stage_0098_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        playerRef.SetValue(BS02_MQ03_Blue_AllowPWEntry_AV, 0.0)
        playerRef.SetValue(BS02_MQ03_Blue_CluesCount_NotSaved, 0.0)
        playerRef.SetValue(BS02_MQ03_Blue_DeniedRahmani_AV, 0.0)
    EndIf

    ObjectReference rahmaniRef = Alias_actor_Rahmani_Atlas.GetReference()
    ObjectReference rahmaniMarker = Alias_xmarker_AtlasRahmaniSpot.GetReference()
    If rahmaniRef != None && rahmaniMarker != None
        rahmaniRef.MoveTo(rahmaniMarker)
    EndIf

    ObjectReference knappRef = Alias_actor_Knapp_Atlas.GetReference()
    ObjectReference knappMarker = Alias_xmarker_AtlasArtSpot.GetReference()
    If knappRef != None && knappMarker != None
        knappRef.MoveTo(knappMarker)
    EndIf

    SetObjectiveDisplayed(10)
    If BS02_MQ03_Atlas_ArtKnappRahmani_NoPlayer != None && !BS02_MQ03_Atlas_ArtKnappRahmani_NoPlayer.IsPlaying()
        BS02_MQ03_Atlas_ArtKnappRahmani_NoPlayer.Start()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    If BS02_MQ03_Atlas_ArtKnappRahmani_NoPlayer != None && !BS02_MQ03_Atlas_ArtKnappRahmani_NoPlayer.IsPlaying()
        BS02_MQ03_Atlas_ArtKnappRahmani_NoPlayer.Start()
    EndIf
EndFunction

Function Fragment_Stage_0120_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)

    If BS02_MQ03_Atlas_ArtKnappRahmani_NoPlayer != None && BS02_MQ03_Atlas_ArtKnappRahmani_NoPlayer.IsPlaying()
        BS02_MQ03_Atlas_ArtKnappRahmani_NoPlayer.Stop()
    EndIf
    If BS02_MQ03_BRCOffice_Rahmani_TravelToOffice != None && !BS02_MQ03_BRCOffice_Rahmani_TravelToOffice.IsPlaying()
        BS02_MQ03_BRCOffice_Rahmani_TravelToOffice.Start()
    EndIf
    If !IsStageDone(200)
        SetStage(200)
    EndIf
EndFunction

Function Fragment_Stage_0205_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)
    If BS02_MQ03_BRCOffice_Rahmani_TravelToOffice != None && BS02_MQ03_BRCOffice_Rahmani_TravelToOffice.IsPlaying()
        BS02_MQ03_BRCOffice_Rahmani_TravelToOffice.Stop()
    EndIf
    If BS02_MQ03_BRCOffice_Drink != None && !BS02_MQ03_BRCOffice_Drink.IsPlaying()
        BS02_MQ03_BRCOffice_Drink.Start()
    EndIf
EndFunction

Function Fragment_Stage_0220_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        playerRef.SetValue(BS02_MQ03_Blue_Barstool_AV, 0.0)
    EndIf
    SetObjectiveDisplayed(35)
EndFunction

Function Fragment_Stage_0225_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        playerRef.SetValue(BS02_MQ03_Blue_DeniedRahmani_AV, 1.0)
    EndIf
    SetObjectiveDisplayed(35, False)
    If !IsStageDone(240)
        SetStage(240)
    EndIf
EndFunction

Function Fragment_Stage_0230_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveCompleted(35)
    SetObjectiveDisplayed(37)
EndFunction

Function Fragment_Stage_0240_Item_00()
    SetObjectiveCompleted(37)
    If !IsStageDone(250)
        SetStage(250)
    EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(35, False)
    SetObjectiveCompleted(37)
    SetObjectiveDisplayed(40)
    If BS02_MQ03_BRCOffice_Drink != None && BS02_MQ03_BRCOffice_Drink.IsPlaying()
        BS02_MQ03_BRCOffice_Drink.Stop()
    EndIf
    If BS02_MQ03_BRCOffice_Rahmani_TravelToOffice != None && !BS02_MQ03_BRCOffice_Rahmani_TravelToOffice.IsPlaying()
        BS02_MQ03_BRCOffice_Rahmani_TravelToOffice.Start()
    EndIf
EndFunction

Function Fragment_Stage_0255_Item_00()
    If BS02_MQ03_BRCOffice_Rahmani_TravelToOffice != None && !BS02_MQ03_BRCOffice_Rahmani_TravelToOffice.IsPlaying()
        BS02_MQ03_BRCOffice_Rahmani_TravelToOffice.Start()
    EndIf
EndFunction

Function Fragment_Stage_0260_Item_00()
    If BS02_MQ03_BRCOffice_RahmaniAndJoanna != None && !BS02_MQ03_BRCOffice_RahmaniAndJoanna.IsPlaying()
        BS02_MQ03_BRCOffice_RahmaniAndJoanna.Start()
    EndIf
EndFunction

Function Fragment_Stage_0265_Item_00()
    If BS02_MQ03_BRCOffice_RahmaniAndJoanna != None && !BS02_MQ03_BRCOffice_RahmaniAndJoanna.IsPlaying()
        BS02_MQ03_BRCOffice_RahmaniAndJoanna.Start()
    EndIf
EndFunction

Function Fragment_Stage_0270_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        playerRef.AddKeyword(BS02_MQ03_Blue_Joanna_AllowTalking)
    EndIf
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(45)
    If BS02_MQ03_BRCOffice_RahmaniAndJoanna != None && BS02_MQ03_BRCOffice_RahmaniAndJoanna.IsPlaying()
        BS02_MQ03_BRCOffice_RahmaniAndJoanna.Stop()
    EndIf
    If BS02_MQ03_BRCOffice_Interview != None && !BS02_MQ03_BRCOffice_Interview.IsPlaying()
        BS02_MQ03_BRCOffice_Interview.Start()
    EndIf
EndFunction

Function Fragment_Stage_0271_Item_00()
    If BS02_MQ03_BRCOffice_Interview != None && !BS02_MQ03_BRCOffice_Interview.IsPlaying()
        BS02_MQ03_BRCOffice_Interview.Start()
    EndIf
EndFunction

Function Fragment_Stage_0280_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        playerRef.RemoveKeyword(BS02_MQ03_Blue_Joanna_AllowTalking)
    EndIf
    SetObjectiveCompleted(45)
    SetObjectiveDisplayed(50)
    If mapmarker_HFD != None
        mapmarker_HFD.AddToMap()
    EndIf
    If BS02_MQ03_BRCOffice_Interview != None && BS02_MQ03_BRCOffice_Interview.IsPlaying()
        BS02_MQ03_BRCOffice_Interview.Stop()
    EndIf
    If !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    Int refIndex = 0
    ObjectReference targetRef
    While refIndex < refcol_EnableForQuest.GetCount()
        targetRef = refcol_EnableForQuest.GetAt(refIndex)
        If targetRef != None && targetRef.IsDisabled()
            targetRef.Enable()
        EndIf
        refIndex += 1
    EndWhile

    refIndex = 0
    While refIndex < refcol_DisableForQuest.GetCount()
        targetRef = refcol_DisableForQuest.GetAt(refIndex)
        If targetRef != None && !targetRef.IsDisabled()
            targetRef.Disable()
        EndIf
        refIndex += 1
    EndWhile

    refIndex = 0
    While refIndex < refcol_BossDeadEnable.GetCount()
        targetRef = refcol_BossDeadEnable.GetAt(refIndex)
        If targetRef != None && !targetRef.IsDisabled()
            targetRef.Disable()
        EndIf
        refIndex += 1
    EndWhile

    ObjectReference bossRef = Alias_boss_Imposterling.GetReference()
    If bossRef != None && !bossRef.IsDisabled()
        bossRef.Disable()
    EndIf

    ObjectReference securityDoor = Alias_door_SecurityDoor.GetReference()
    If securityDoor != None
        securityDoor.SetOpen(False)
        securityDoor.Lock(True)
    EndIf

    ObjectReference weaponDoor = Alias_door_WeaponDoor.GetReference()
    If weaponDoor != None
        weaponDoor.SetOpen(False)
        weaponDoor.Lock(True)
    EndIf

    ObjectReference wallDoor = Alias_door_WallDoor.GetReference()
    If wallDoor != None
        wallDoor.SetOpen(False)
        wallDoor.Lock(True)
    EndIf

    ObjectReference clueDisplay = Alias_note_CampManifest_CannotTake.GetReference()
    If clueDisplay != None && !clueDisplay.IsDisabled()
        clueDisplay.Disable()
    EndIf
    clueDisplay = Alias_note_CampRadstorms_CannotTake.GetReference()
    If clueDisplay != None && !clueDisplay.IsDisabled()
        clueDisplay.Disable()
    EndIf
    clueDisplay = Alias_note_CampJournal_CannotTake.GetReference()
    If clueDisplay != None && !clueDisplay.IsDisabled()
        clueDisplay.Disable()
    EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(52)
    If BS02_MQ03_Tunnel_z_Comments_PreAries != None && !BS02_MQ03_Tunnel_z_Comments_PreAries.IsPlaying()
        BS02_MQ03_Tunnel_z_Comments_PreAries.Start()
    EndIf
EndFunction

Function Fragment_Stage_0375_Item_00()
    Actor rahmaniRef = Alias_actor_Rahmani_Pipeline.GetActorReference()
    If rahmaniRef != None
        rahmaniRef.SetValue(BS02_MQ03_Blue_RahmaniFollower_AV, 1.0)
        rahmaniRef.EvaluatePackage()
    EndIf
    SetObjectiveCompleted(52)
    SetObjectiveDisplayed(54)
EndFunction

Function Fragment_Stage_0377_Item_00()
    SetObjectiveCompleted(54)
    SetObjectiveDisplayed(55)
EndFunction

Function Fragment_Stage_0380_Item_00()
    Actor ariesRef = Alias_actor_Aries_Pipeline.GetActorReference()
    If ariesRef != None
        ariesRef.SetValue(BS02_MQ03_Blue_AriesFollower_AV, 1.0)
        ariesRef.EvaluatePackage()
    EndIf
    SetObjectiveCompleted(55)
    SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0390_Item_00()
    If BS02_MQ03_Tunnel_AriesIntro_2 != None && !BS02_MQ03_Tunnel_AriesIntro_2.IsPlaying()
        BS02_MQ03_Tunnel_AriesIntro_2.Start()
    EndIf
EndFunction

Function Fragment_Stage_0395_Item_00()
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(65)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(65)
    SetObjectiveDisplayed(67)
    If BS02_MQ03_Tunnel_AriesIntro_2 != None && BS02_MQ03_Tunnel_AriesIntro_2.IsPlaying()
        BS02_MQ03_Tunnel_AriesIntro_2.Stop()
    EndIf
    If BS02_MQ03_Tunnel_z_Comments_PostAries != None && !BS02_MQ03_Tunnel_z_Comments_PostAries.IsPlaying()
        BS02_MQ03_Tunnel_z_Comments_PostAries.Start()
    EndIf
EndFunction

Function Fragment_Stage_0410_Item_00()
    ObjectReference rahmaniRef = Alias_actor_Rahmani_Pipeline.GetReference()
    ObjectReference observationMarker = alias_xmarker_Teleport_Observation.GetReference()
    If rahmaniRef != None && observationMarker != None
        rahmaniRef.MoveTo(observationMarker)
    EndIf
    SetObjectiveCompleted(67)
    SetObjectiveDisplayed(69)
    SetObjectiveDisplayed(900)
    SetObjectiveDisplayed(901)
EndFunction

Function Fragment_Stage_0420_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        playerRef.SetValue(BS02_MQ03_Blue_AllowPWEntry_AV, 1.0)
    EndIf
    SetObjectiveDisplayed(900, False)
    SetObjectiveDisplayed(901, False)
    If BS02_MQ03_Tunnel_z_Comments_AllDone != None && !BS02_MQ03_Tunnel_z_Comments_AllDone.IsPlaying()
        BS02_MQ03_Tunnel_z_Comments_AllDone.Start()
    EndIf
EndFunction

Function Fragment_Stage_0421_Item_00()
    If BS02_MQ03_Tunnel_z_Comments_PasswordComment1 != None && !BS02_MQ03_Tunnel_z_Comments_PasswordComment1.IsPlaying()
        BS02_MQ03_Tunnel_z_Comments_PasswordComment1.Start()
    EndIf
    If IsStageDone(422) && IsStageDone(423) && !IsStageDone(420)
        SetStage(420)
    EndIf
EndFunction

Function Fragment_Stage_0422_Item_00()
    If BS02_MQ03_Tunnel_z_Comments_PasswordComment2 != None && !BS02_MQ03_Tunnel_z_Comments_PasswordComment2.IsPlaying()
        BS02_MQ03_Tunnel_z_Comments_PasswordComment2.Start()
    EndIf
    If IsStageDone(421) && IsStageDone(423) && !IsStageDone(420)
        SetStage(420)
    EndIf
EndFunction

Function Fragment_Stage_0423_Item_00()
    If BS02_MQ03_Tunnel_z_Comments_PasswordComment3 != None && !BS02_MQ03_Tunnel_z_Comments_PasswordComment3.IsPlaying()
        BS02_MQ03_Tunnel_z_Comments_PasswordComment3.Start()
    EndIf
    If IsStageDone(421) && IsStageDone(422) && !IsStageDone(420)
        SetStage(420)
    EndIf
EndFunction

Function Fragment_Stage_0440_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        playerRef.AddKeyword(BS02_MQ03_Blue_DoorOpened)
    EndIf

    ObjectReference securityDoor = Alias_door_SecurityDoor.GetReference()
    If securityDoor != None
        securityDoor.Lock(False)
        securityDoor.SetOpen(True)
    EndIf
    ObjectReference weaponDoor = Alias_door_WeaponDoor.GetReference()
    If weaponDoor != None
        weaponDoor.Lock(False)
        weaponDoor.SetOpen(True)
    EndIf

    SetObjectiveCompleted(69)
    SetObjectiveDisplayed(70)
    If BS02_MQ03_Tunnel_z_Comments_LookAtWeapon != None && !BS02_MQ03_Tunnel_z_Comments_LookAtWeapon.IsPlaying()
        BS02_MQ03_Tunnel_z_Comments_LookAtWeapon.Start()
    EndIf
EndFunction

Function Fragment_Stage_0445_Item_00()
    If BS02_MQ03_Tunnel_z_Comments_PasswordComment1 != None && BS02_MQ03_Tunnel_z_Comments_PasswordComment1.IsPlaying()
        BS02_MQ03_Tunnel_z_Comments_PasswordComment1.Stop()
    EndIf
    If BS02_MQ03_Tunnel_z_Comments_PasswordComment2 != None && BS02_MQ03_Tunnel_z_Comments_PasswordComment2.IsPlaying()
        BS02_MQ03_Tunnel_z_Comments_PasswordComment2.Stop()
    EndIf
    If BS02_MQ03_Tunnel_z_Comments_PasswordComment3 != None && BS02_MQ03_Tunnel_z_Comments_PasswordComment3.IsPlaying()
        BS02_MQ03_Tunnel_z_Comments_PasswordComment3.Stop()
    EndIf
    If BS02_MQ03_Tunnel_z_Comments_AllDone != None && BS02_MQ03_Tunnel_z_Comments_AllDone.IsPlaying()
        BS02_MQ03_Tunnel_z_Comments_AllDone.Stop()
    EndIf
EndFunction

Function Fragment_Stage_0450_Item_00()
    If BS02_MQ03_Tunnel_RahmaniWeaponInvestigate != None && !BS02_MQ03_Tunnel_RahmaniWeaponInvestigate.IsPlaying()
        BS02_MQ03_Tunnel_RahmaniWeaponInvestigate.Start()
    EndIf
EndFunction

Function Fragment_Stage_0455_Item_00()
    ObjectReference weaponProp = Alias_static_BrokenWeapon.GetReference()
    If weaponProp != None && !weaponProp.IsDisabled()
        weaponProp.Disable()
    EndIf
    If BS02_MQ03_Tunnel_z_Comments_WeaponPickUp != None && !BS02_MQ03_Tunnel_z_Comments_WeaponPickUp.IsPlaying()
        BS02_MQ03_Tunnel_z_Comments_WeaponPickUp.Start()
    EndIf
EndFunction

Function Fragment_Stage_0460_Item_00()
    SetObjectiveCompleted(70)
    SetObjectiveDisplayed(75)
    If BS02_MQ03_Tunnel_RahmaniWeaponInvestigate != None && BS02_MQ03_Tunnel_RahmaniWeaponInvestigate.IsPlaying()
        BS02_MQ03_Tunnel_RahmaniWeaponInvestigate.Stop()
    EndIf
    If BS02_MQ03_Tunnel_z_Comments_ChargingStationTravel != None && !BS02_MQ03_Tunnel_z_Comments_ChargingStationTravel.IsPlaying()
        BS02_MQ03_Tunnel_z_Comments_ChargingStationTravel.Start()
    EndIf
EndFunction

Function Fragment_Stage_0470_Item_00()
    If BS02_MQ03_Tunnel_z_Comments_ChargingStationTravel != None && !BS02_MQ03_Tunnel_z_Comments_ChargingStationTravel.IsPlaying()
        BS02_MQ03_Tunnel_z_Comments_ChargingStationTravel.Start()
    EndIf
EndFunction

Function Fragment_Stage_0477_Item_00()
    SetObjectiveCompleted(75)
    If BS02_MQ03_Tunnel_Rahmani_EWS_FIGHT != None && !BS02_MQ03_Tunnel_Rahmani_EWS_FIGHT.IsPlaying()
        BS02_MQ03_Tunnel_Rahmani_EWS_FIGHT.Start()
    EndIf
EndFunction

Function Fragment_Stage_0479_Item_00()
    Quests:_Default:ProgressBar:MasterScript progressScript = (Self as Quest) as Quests:_Default:ProgressBar:MasterScript
    If progressScript != None
        progressScript.CurrPercentage = 0.0
    EndIf
    SetObjectiveDisplayed(77)
EndFunction

Function Fragment_Stage_0480_Item_00()
    SetObjectiveDisplayed(80)
    If BS02_MQ03_Tunnel_Rahmani_EWS_FIGHT != None && !BS02_MQ03_Tunnel_Rahmani_EWS_FIGHT.IsPlaying()
        BS02_MQ03_Tunnel_Rahmani_EWS_FIGHT.Start()
    EndIf

    Actor playerRef = Alias_Player.GetActorReference()
    ActorBase waveActorBase = Game.GetFormFromFile(0x0052C788, "SeventySix.esm") as ActorBase
    ReferenceAlias spawnAlias = GetAlias(133) as ReferenceAlias
    RefCollectionAlias waveEnemies = GetAlias(134) as RefCollectionAlias
    ObjectReference spawnCenter
    If spawnAlias != None
        spawnCenter = spawnAlias.GetReference()
    EndIf
    If playerRef == None || waveActorBase == None || spawnCenter == None || waveEnemies == None
        Return
    EndIf

    Actor[] spawnedWave = new Actor[1]
    Int spawnIndex = 0
    While spawnIndex < spawnedWave.Length
        Actor spawnedActor = spawnCenter.PlaceAtMe(waveActorBase) as Actor
        If spawnedActor == None
            Return
        EndIf
        spawnedWave[spawnIndex] = spawnedActor
        waveEnemies.AddRef(spawnedActor)
        spawnedActor.StartCombat(playerRef, True)
        spawnIndex += 1
    EndWhile

    Bool anyAlive = True
    While anyAlive
        anyAlive = False
        Int actorIndex = 0
        While actorIndex < spawnedWave.Length
            Actor waveActor = spawnedWave[actorIndex]
            If waveActor != None && !waveActor.IsDead()
                anyAlive = True
            EndIf
            actorIndex += 1
        EndWhile
        If anyAlive
            Utility.Wait(1.0)
        EndIf
    EndWhile

    Int cleanupIndex = 0
    While cleanupIndex < spawnedWave.Length
        waveEnemies.RemoveRef(spawnedWave[cleanupIndex])
        cleanupIndex += 1
    EndWhile
    If !IsStageDone(481)
        SetStage(481)
    EndIf
EndFunction

Function Fragment_Stage_0481_Item_00()
    If !IsStageDone(486)
        SetStage(486)
    EndIf

    Actor playerRef = Alias_Player.GetActorReference()
    ActorBase waveActorBase = Game.GetFormFromFile(0x0052C788, "SeventySix.esm") as ActorBase
    ReferenceAlias spawnAlias = GetAlias(133) as ReferenceAlias
    RefCollectionAlias waveEnemies = GetAlias(134) as RefCollectionAlias
    ObjectReference spawnCenter
    If spawnAlias != None
        spawnCenter = spawnAlias.GetReference()
    EndIf
    If playerRef == None || waveActorBase == None || spawnCenter == None || waveEnemies == None
        Return
    EndIf

    Actor[] spawnedWave = new Actor[1]
    Int spawnIndex = 0
    While spawnIndex < spawnedWave.Length
        Actor spawnedActor = spawnCenter.PlaceAtMe(waveActorBase) as Actor
        If spawnedActor == None
            Return
        EndIf
        spawnedWave[spawnIndex] = spawnedActor
        waveEnemies.AddRef(spawnedActor)
        spawnedActor.StartCombat(playerRef, True)
        spawnIndex += 1
    EndWhile

    Bool anyAlive = True
    While anyAlive
        anyAlive = False
        Int actorIndex = 0
        While actorIndex < spawnedWave.Length
            Actor waveActor = spawnedWave[actorIndex]
            If waveActor != None && !waveActor.IsDead()
                anyAlive = True
            EndIf
            actorIndex += 1
        EndWhile
        If anyAlive
            Utility.Wait(1.0)
        EndIf
    EndWhile

    Int cleanupIndex = 0
    While cleanupIndex < spawnedWave.Length
        waveEnemies.RemoveRef(spawnedWave[cleanupIndex])
        cleanupIndex += 1
    EndWhile
    If !IsStageDone(482)
        SetStage(482)
    EndIf
EndFunction

Function Fragment_Stage_0482_Item_00()
    If !IsStageDone(487)
        SetStage(487)
    EndIf

    Actor playerRef = Alias_Player.GetActorReference()
    ActorBase waveActorBase = Game.GetFormFromFile(0x0052C788, "SeventySix.esm") as ActorBase
    ReferenceAlias spawnAlias = GetAlias(133) as ReferenceAlias
    RefCollectionAlias waveEnemies = GetAlias(134) as RefCollectionAlias
    ObjectReference spawnCenter
    If spawnAlias != None
        spawnCenter = spawnAlias.GetReference()
    EndIf
    If playerRef == None || waveActorBase == None || spawnCenter == None || waveEnemies == None
        Return
    EndIf

    Actor[] spawnedWave = new Actor[1]
    Int spawnIndex = 0
    While spawnIndex < spawnedWave.Length
        Actor spawnedActor = spawnCenter.PlaceAtMe(waveActorBase) as Actor
        If spawnedActor == None
            Return
        EndIf
        spawnedWave[spawnIndex] = spawnedActor
        waveEnemies.AddRef(spawnedActor)
        spawnedActor.StartCombat(playerRef, True)
        spawnIndex += 1
    EndWhile

    Bool anyAlive = True
    While anyAlive
        anyAlive = False
        Int actorIndex = 0
        While actorIndex < spawnedWave.Length
            Actor waveActor = spawnedWave[actorIndex]
            If waveActor != None && !waveActor.IsDead()
                anyAlive = True
            EndIf
            actorIndex += 1
        EndWhile
        If anyAlive
            Utility.Wait(1.0)
        EndIf
    EndWhile

    Int cleanupIndex = 0
    While cleanupIndex < spawnedWave.Length
        waveEnemies.RemoveRef(spawnedWave[cleanupIndex])
        cleanupIndex += 1
    EndWhile
    If !IsStageDone(483)
        SetStage(483)
    EndIf
EndFunction

Function Fragment_Stage_0483_Item_00()
    If !IsStageDone(488)
        SetStage(488)
    EndIf

    Actor playerRef = Alias_Player.GetActorReference()
    ActorBase waveActorBase = Game.GetFormFromFile(0x0052C788, "SeventySix.esm") as ActorBase
    ReferenceAlias spawnAlias = GetAlias(133) as ReferenceAlias
    RefCollectionAlias waveEnemies = GetAlias(134) as RefCollectionAlias
    ObjectReference spawnCenter
    If spawnAlias != None
        spawnCenter = spawnAlias.GetReference()
    EndIf
    If playerRef == None || waveActorBase == None || spawnCenter == None || waveEnemies == None
        Return
    EndIf

    Actor[] spawnedWave = new Actor[1]
    Int spawnIndex = 0
    While spawnIndex < spawnedWave.Length
        Actor spawnedActor = spawnCenter.PlaceAtMe(waveActorBase) as Actor
        If spawnedActor == None
            Return
        EndIf
        spawnedWave[spawnIndex] = spawnedActor
        waveEnemies.AddRef(spawnedActor)
        spawnedActor.StartCombat(playerRef, True)
        spawnIndex += 1
    EndWhile

    Bool anyAlive = True
    While anyAlive
        anyAlive = False
        Int actorIndex = 0
        While actorIndex < spawnedWave.Length
            Actor waveActor = spawnedWave[actorIndex]
            If waveActor != None && !waveActor.IsDead()
                anyAlive = True
            EndIf
            actorIndex += 1
        EndWhile
        If anyAlive
            Utility.Wait(1.0)
        EndIf
    EndWhile

    Int cleanupIndex = 0
    While cleanupIndex < spawnedWave.Length
        waveEnemies.RemoveRef(spawnedWave[cleanupIndex])
        cleanupIndex += 1
    EndWhile
    If !IsStageDone(484)
        SetStage(484)
    EndIf
EndFunction

Function Fragment_Stage_0484_Item_00()
    If !IsStageDone(489)
        SetStage(489)
    EndIf
    If !IsStageDone(485)
        SetStage(485)
    EndIf
EndFunction

Function Fragment_Stage_0485_Item_00()
    ObjectReference alarmButton = activator_KlaxonButton_Hidden.GetReference()
    If alarmButton != None && !alarmButton.IsDisabled()
        alarmButton.Disable()
    EndIf
EndFunction

Function Fragment_Stage_0486_Item_00()
    Quests:_Default:ProgressBar:MasterScript progressScript = (Self as Quest) as Quests:_Default:ProgressBar:MasterScript
    If progressScript != None
        progressScript.CurrPercentage = 25.0
    EndIf
    SetObjectiveDisplayed(77, True, True)
EndFunction

Function Fragment_Stage_0487_Item_00()
    Quests:_Default:ProgressBar:MasterScript progressScript = (Self as Quest) as Quests:_Default:ProgressBar:MasterScript
    If progressScript != None
        progressScript.CurrPercentage = 50.0
    EndIf
    SetObjectiveDisplayed(77, True, True)
EndFunction

Function Fragment_Stage_0488_Item_00()
    Quests:_Default:ProgressBar:MasterScript progressScript = (Self as Quest) as Quests:_Default:ProgressBar:MasterScript
    If progressScript != None
        progressScript.CurrPercentage = 75.0
    EndIf
    SetObjectiveDisplayed(77, True, True)
EndFunction

Function Fragment_Stage_0489_Item_00()
    Quests:_Default:ProgressBar:MasterScript progressScript = (Self as Quest) as Quests:_Default:ProgressBar:MasterScript
    If progressScript != None
        progressScript.CurrPercentage = 100.0
    EndIf
    SetObjectiveCompleted(77)
    SetObjectiveCompleted(80)
    SetObjectiveDisplayed(85)
    If !IsStageDone(490)
        SetStage(490)
    EndIf
EndFunction

Function Fragment_Stage_0490_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        playerRef.AddKeyword(BS02_MQ03_Blue_AllowVineDestroy)
    EndIf
    If BS02_MQ03_Tunnel_Rahmani_EWS_FIGHT != None && BS02_MQ03_Tunnel_Rahmani_EWS_FIGHT.IsPlaying()
        BS02_MQ03_Tunnel_Rahmani_EWS_FIGHT.Stop()
    EndIf
    If BS02_MQ03_Tunnel_z_Comments_CutVines != None && !BS02_MQ03_Tunnel_z_Comments_CutVines.IsPlaying()
        BS02_MQ03_Tunnel_z_Comments_CutVines.Start()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(85)
    SetObjectiveDisplayed(90)
EndFunction

Function Fragment_Stage_0505_Item_00()
    If BS02_MQ03_Tunnel_z_Comments_SheepsquatchRoar != None && !BS02_MQ03_Tunnel_z_Comments_SheepsquatchRoar.IsPlaying()
        BS02_MQ03_Tunnel_z_Comments_SheepsquatchRoar.Start()
    EndIf
EndFunction

Function Fragment_Stage_0510_Item_00()
    If BS02_MQ03_Tunnel_z_Comments_ReadTerminal != None && !BS02_MQ03_Tunnel_z_Comments_ReadTerminal.IsPlaying()
        BS02_MQ03_Tunnel_z_Comments_ReadTerminal.Start()
    EndIf
EndFunction

Function Fragment_Stage_0520_Item_00()
    SetObjectiveCompleted(90)
    SetObjectiveDisplayed(95)
    ObjectReference bossRef = Alias_boss_Imposterling.GetReference()
    If bossRef != None && bossRef.IsDisabled()
        bossRef.Enable()
    EndIf
EndFunction

Function Fragment_Stage_0530_Item_00()
    SetObjectiveCompleted(95)
    SetObjectiveDisplayed(100)
    Int refIndex = 0
    While refIndex < refcol_BossDeadEnable.GetCount()
        ObjectReference targetRef = refcol_BossDeadEnable.GetAt(refIndex)
        If targetRef != None && targetRef.IsDisabled()
            targetRef.Enable()
        EndIf
        refIndex += 1
    EndWhile
    If BS02_MQ03_Tunnel_z_Comments_KilledBoss != None && !BS02_MQ03_Tunnel_z_Comments_KilledBoss.IsPlaying()
        BS02_MQ03_Tunnel_z_Comments_KilledBoss.Start()
    EndIf
EndFunction

Function Fragment_Stage_0535_Item_00()
    SetObjectiveDisplayed(100)
    If BS02_MQ03_Tunnel_z_Comments_MissedTerminal != None && !BS02_MQ03_Tunnel_z_Comments_MissedTerminal.IsPlaying()
        BS02_MQ03_Tunnel_z_Comments_MissedTerminal.Start()
    EndIf
EndFunction

Function Fragment_Stage_0540_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(110)
    If BS02_MQ03_Tunnel_z_Comments_MissedTerminal != None && BS02_MQ03_Tunnel_z_Comments_MissedTerminal.IsPlaying()
        BS02_MQ03_Tunnel_z_Comments_MissedTerminal.Stop()
    EndIf
    If BS02_MQ03_Tunnel_AriesTerminal != None && !BS02_MQ03_Tunnel_AriesTerminal.IsPlaying()
        BS02_MQ03_Tunnel_AriesTerminal.Start()
    EndIf
EndFunction

Function Fragment_Stage_0560_Item_00()
    SetObjectiveCompleted(110)
    SetObjectiveDisplayed(115)
    If BS02_MQ03_Tunnel_AriesTerminal != None && !BS02_MQ03_Tunnel_AriesTerminal.IsPlaying()
        BS02_MQ03_Tunnel_AriesTerminal.Start()
    EndIf
EndFunction

Function Fragment_Stage_0570_Item_00()
    ObjectReference wallDoor = Alias_door_WallDoor.GetReference()
    If wallDoor != None
        wallDoor.Lock(False)
        wallDoor.SetOpen(True)
    EndIf
    ObjectReference wallButton = activator_WallButton_Hidden.GetReference()
    If wallButton != None && wallButton.IsDisabled()
        wallButton.Enable()
    EndIf
    SetObjectiveCompleted(115)
    SetObjectiveDisplayed(120)
EndFunction

Function Fragment_Stage_0600_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        playerRef.SetValue(BS02_MQ03_Blue_CluesCount_NotSaved, 0.0)
    EndIf

    ObjectReference rahmaniRef = Alias_actor_Rahmani_Pipeline.GetReference()
    ObjectReference rahmaniMarker = Alias_xmarker_CampRahmani.GetReference()
    If rahmaniRef != None && rahmaniMarker != None
        rahmaniRef.MoveTo(rahmaniMarker)
    EndIf
    ObjectReference ariesRef = Alias_actor_Aries_Pipeline.GetReference()
    ObjectReference ariesMarker = Alias_xmarker_CampAries.GetReference()
    If ariesRef != None && ariesMarker != None
        ariesRef.MoveTo(ariesMarker)
    EndIf

    SetObjectiveCompleted(120)
    SetObjectiveDisplayed(130)
    SetObjectiveDisplayed(135)
EndFunction

Function Fragment_Stage_0610_Item_00()
    SetObjectiveCompleted(130)
    SetObjectiveCompleted(135)
    SetObjectiveDisplayed(140)
EndFunction

Function Fragment_Stage_0611_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        playerRef.SetValue(BS02_MQ03_Blue_CluesCount_NotSaved, playerRef.GetValue(BS02_MQ03_Blue_CluesCount_NotSaved) + 1.0)
    EndIf
    SetObjectiveDisplayed(135, True, True)
    If IsStageDone(612) && IsStageDone(613) && !IsStageDone(610)
        SetStage(610)
    EndIf
EndFunction

Function Fragment_Stage_0612_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        playerRef.SetValue(BS02_MQ03_Blue_CluesCount_NotSaved, playerRef.GetValue(BS02_MQ03_Blue_CluesCount_NotSaved) + 1.0)
    EndIf
    SetObjectiveDisplayed(135, True, True)
    If IsStageDone(611) && IsStageDone(613) && !IsStageDone(610)
        SetStage(610)
    EndIf
EndFunction

Function Fragment_Stage_0613_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        playerRef.SetValue(BS02_MQ03_Blue_CluesCount_NotSaved, playerRef.GetValue(BS02_MQ03_Blue_CluesCount_NotSaved) + 1.0)
    EndIf
    SetObjectiveDisplayed(135, True, True)
    If IsStageDone(611) && IsStageDone(612) && !IsStageDone(610)
        SetStage(610)
    EndIf
EndFunction

Function Fragment_Stage_0620_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        playerRef.RemoveItem(CampClue1_Base, 1, True)
        playerRef.RemoveItem(CampClue2_Base, 1, True)
        playerRef.RemoveItem(CampClue3_Base, 1, True)
    EndIf
    SetObjectiveCompleted(140)
    SetObjectiveDisplayed(145)
    If BS02_MQ03_Tunnel_Rahmani_ReviewEvidence != None && !BS02_MQ03_Tunnel_Rahmani_ReviewEvidence.IsPlaying()
        BS02_MQ03_Tunnel_Rahmani_ReviewEvidence.Start()
    EndIf
EndFunction

Function Fragment_Stage_0625_Item_00()
    SetObjectiveCompleted(145)
    SetObjectiveDisplayed(150)
EndFunction

Function Fragment_Stage_0626_Item_00()
    Actor ariesRef = Alias_actor_Aries_Pipeline.GetActorReference()
    If ariesRef != None
        ariesRef.SetValue(BS02_MQ03_Blue_AriesFollower_AV, 0.0)
        ariesRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0627_Item_00()
    ObjectReference clueDisplay = Alias_note_CampManifest_CannotTake.GetReference()
    If clueDisplay != None && clueDisplay.IsDisabled()
        clueDisplay.Enable()
    EndIf
    clueDisplay = Alias_note_CampRadstorms_CannotTake.GetReference()
    If clueDisplay != None && clueDisplay.IsDisabled()
        clueDisplay.Enable()
    EndIf
    clueDisplay = Alias_note_CampJournal_CannotTake.GetReference()
    If clueDisplay != None && clueDisplay.IsDisabled()
        clueDisplay.Enable()
    EndIf
EndFunction

Function Fragment_Stage_0640_Item_00()
    SetObjectiveCompleted(150)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_8000_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        playerRef.AddKeyword(BS02_MQ03_Blue_AllowVineDestroy)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf

    Actor rahmaniRef = Alias_actor_Rahmani_Pipeline.GetActorReference()
    If rahmaniRef != None
        rahmaniRef.SetValue(BS02_MQ03_Blue_RahmaniFollower_AV, 0.0)
        rahmaniRef.EvaluatePackage()
    EndIf
    Actor ariesRef = Alias_actor_Aries_Pipeline.GetActorReference()
    If ariesRef != None
        ariesRef.SetValue(BS02_MQ03_Blue_AriesFollower_AV, 0.0)
        ariesRef.EvaluatePackage()
    EndIf

    If BS02_MQ03_Tunnel_Rahmani_ReviewEvidence != None && BS02_MQ03_Tunnel_Rahmani_ReviewEvidence.IsPlaying()
        BS02_MQ03_Tunnel_Rahmani_ReviewEvidence.Stop()
    EndIf
    If BS02_MQ03_Tunnel_z_PostQuest_Raries != None && !BS02_MQ03_Tunnel_z_PostQuest_Raries.IsPlaying()
        BS02_MQ03_Tunnel_z_PostQuest_Raries.Start()
    EndIf

    If playerRef != None && BS02_MQ04_Conscience != None && BS02_MQ04_Conscience_StartKeyword != None
        If !BS02_MQ04_Conscience.IsRunning() && !BS02_MQ04_Conscience.IsCompleted()
            BS02_MQ04_Conscience_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        EndIf

        If BS02_MQ04_Conscience.IsRunning() || BS02_MQ04_Conscience.IsCompleted()
            Quests:BS02_MQ03_Blue:PostQuestExitInstance exitScript = Alias_Player as Quests:BS02_MQ03_Blue:PostQuestExitInstance
            If exitScript != None
                exitScript.ReconcileSuccessorAcceptance()
            EndIf
        ElseIf IsStageDone(9000) && !IsStageDone(9999)
            StartTimer(5.0, 9000)
        EndIf
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != 9000 || !IsStageDone(9000) || IsStageDone(9999) || BS02_MQ04_Conscience == None || BS02_MQ04_Conscience_StartKeyword == None
        Return
    EndIf

    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        StartTimer(5.0, 9000)
        Return
    EndIf

    If !BS02_MQ04_Conscience.IsRunning() && !BS02_MQ04_Conscience.IsCompleted()
        BS02_MQ04_Conscience_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
    EndIf

    If BS02_MQ04_Conscience.IsRunning() || BS02_MQ04_Conscience.IsCompleted()
        Quests:BS02_MQ03_Blue:PostQuestExitInstance exitScript = Alias_Player as Quests:BS02_MQ03_Blue:PostQuestExitInstance
        If exitScript != None
            exitScript.ReconcileSuccessorAcceptance()
        EndIf
    ElseIf IsStageDone(9000) && !IsStageDone(9999)
        StartTimer(5.0, 9000)
    EndIf
EndEvent

Function Fragment_Stage_9010_Item_00()
    If BS02_MQ03_Atlas_ArtKnappRahmani_NoPlayer != None && BS02_MQ03_Atlas_ArtKnappRahmani_NoPlayer.IsPlaying()
        BS02_MQ03_Atlas_ArtKnappRahmani_NoPlayer.Stop()
    EndIf
    If BS02_MQ03_BRCOffice_Rahmani_TravelToOffice != None && BS02_MQ03_BRCOffice_Rahmani_TravelToOffice.IsPlaying()
        BS02_MQ03_BRCOffice_Rahmani_TravelToOffice.Stop()
    EndIf
    If BS02_MQ03_BRCOffice_Drink != None && BS02_MQ03_BRCOffice_Drink.IsPlaying()
        BS02_MQ03_BRCOffice_Drink.Stop()
    EndIf
    If BS02_MQ03_BRCOffice_Interview != None && BS02_MQ03_BRCOffice_Interview.IsPlaying()
        BS02_MQ03_BRCOffice_Interview.Stop()
    EndIf
    If BS02_MQ03_BRCOffice_RahmaniAndJoanna != None && BS02_MQ03_BRCOffice_RahmaniAndJoanna.IsPlaying()
        BS02_MQ03_BRCOffice_RahmaniAndJoanna.Stop()
    EndIf
    If BS02_MQ03_Tunnel_AriesIntro_2 != None && BS02_MQ03_Tunnel_AriesIntro_2.IsPlaying()
        BS02_MQ03_Tunnel_AriesIntro_2.Stop()
    EndIf
    If BS02_MQ03_Tunnel_RahmaniWeaponInvestigate != None && BS02_MQ03_Tunnel_RahmaniWeaponInvestigate.IsPlaying()
        BS02_MQ03_Tunnel_RahmaniWeaponInvestigate.Stop()
    EndIf
    If BS02_MQ03_Tunnel_Rahmani_EWS_FIGHT != None && BS02_MQ03_Tunnel_Rahmani_EWS_FIGHT.IsPlaying()
        BS02_MQ03_Tunnel_Rahmani_EWS_FIGHT.Stop()
    EndIf
    If BS02_MQ03_Tunnel_AriesTerminal != None && BS02_MQ03_Tunnel_AriesTerminal.IsPlaying()
        BS02_MQ03_Tunnel_AriesTerminal.Stop()
    EndIf
    If BS02_MQ03_Tunnel_Rahmani_ReviewEvidence != None && BS02_MQ03_Tunnel_Rahmani_ReviewEvidence.IsPlaying()
        BS02_MQ03_Tunnel_Rahmani_ReviewEvidence.Stop()
    EndIf
    If BS02_MQ03_Tunnel_z_PostQuest_Raries != None && BS02_MQ03_Tunnel_z_PostQuest_Raries.IsPlaying()
        BS02_MQ03_Tunnel_z_PostQuest_Raries.Stop()
    EndIf
EndFunction

Function Fragment_Stage_9999_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        playerRef.RemoveKeyword(BS02_MQ03_Blue_AllowVineDestroy)
        playerRef.RemoveKeyword(BS02_MQ03_Blue_DoorOpened)
        playerRef.RemoveKeyword(BS02_MQ03_Blue_Joanna_AllowTalking)

        ObjectReference passwordClue = PWClue1_Alias.GetReference()
        If passwordClue != None
            playerRef.RemoveItem(passwordClue, 1, True)
        EndIf
        passwordClue = PWClue2_Alias.GetReference()
        If passwordClue != None
            playerRef.RemoveItem(passwordClue, 1, True)
        EndIf
        passwordClue = PWClue3_Alias.GetReference()
        If passwordClue != None
            playerRef.RemoveItem(passwordClue, 1, True)
        EndIf
    EndIf
    If !IsStageDone(9010)
        SetStage(9010)
    EndIf
    Stop()
EndFunction
