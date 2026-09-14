Function SetAmbientStoryManagerMarkersEnabled(Bool abEnable)
    If Alias_EnableMarkers_AmbientSMs == None
        Return
    EndIf
    Int markerIndex = 0
    While markerIndex < Alias_EnableMarkers_AmbientSMs.GetCount()
        ObjectReference markerRef = Alias_EnableMarkers_AmbientSMs.GetAt(markerIndex)
        If markerRef != None
            If abEnable
                markerRef.Enable()
            Else
                markerRef.Disable()
            EndIf
        EndIf
        markerIndex += 1
    EndWhile
EndFunction

Function Fragment_Stage_0001_Item_00()
    SetStage(900)
EndFunction

Function Fragment_Stage_0002_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        playerRef.SetValue(BS02_AV_BrotherhoodLeaderFinal, 1.0)
    EndIf
    SetStage(1500)
EndFunction

Function Fragment_Stage_0003_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        playerRef.SetValue(BS02_AV_BrotherhoodLeaderFinal, 2.0)
    EndIf
    SetStage(1500)
EndFunction

Function Fragment_Stage_0010_Item_00()
    If !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0015_Item_00()
    Alias_Actor_Rahmani_FortAtlas.TryToMoveTo(Alias_Marker_RahmaniIntro.GetReference())
    Alias_Actor_Shin_FortAtlas.TryToMoveTo(Alias_Marker_ShinIntro.GetReference())
    Alias_Actor_Valdez.TryToMoveTo(Alias_Marker_ValdezIntro.GetReference())
    Alias_Actor_Blackburn_FortAtlas.TryToMoveTo(Alias_Marker_BlackburnIntro.GetReference())
EndFunction

Function Fragment_Stage_0020_Item_00()
    If IsStageDone(1475) && !IsStageDone(1500)
        SetStage(1500)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        playerRef.SetValue(BS01_RahmaniAwayValue, 1.0)
        playerRef.SetValue(BS01_ShinAwayValue, 1.0)
        playerRef.SetValue(BS02_AV_BrotherhoodLeaderFinal, 0.0)
        playerRef.SetValue(BS02_AV_BrotherhoodLeaderDead, 0.0)
        playerRef.SetValue(BS02_MQ05_Catalyst_DorseyAwayValue, 0.0)
        playerRef.SetValue(BS02_HewsenAwayValue, 0.0)
    EndIf
    Alias_EnableMarker_BlackburnVault96.TryToDisable()
    Alias_EnableMarker_ValdezVault96.TryToDisable()
    SetAmbientStoryManagerMarkersEnabled(False)
    Alias_EnableMarker_FortAtlasInitiates.TryToDisable()
    Alias_Actor_Rahmani_FortAtlas.TryToMoveTo(Alias_Marker_RahmaniIntro.GetReference())
    Alias_Actor_Shin_FortAtlas.TryToMoveTo(Alias_Marker_ShinIntro.GetReference())
    Alias_Actor_Valdez.TryToMoveTo(Alias_Marker_ValdezIntro.GetReference())
    Alias_Actor_Blackburn_FortAtlas.TryToMoveTo(Alias_Marker_BlackburnIntro.GetReference())
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0150_Item_00()
    If BS02_MQ05_Catalyst_PreIntro != None && !BS02_MQ05_Catalyst_PreIntro.IsPlaying()
        BS02_MQ05_Catalyst_PreIntro.Start()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    If BS02_MQ05_Catalyst_PreIntro != None && BS02_MQ05_Catalyst_PreIntro.IsPlaying()
        BS02_MQ05_Catalyst_PreIntro.Stop()
    EndIf
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(15)
EndFunction

Function Fragment_Stage_0250_Item_00()
    SetObjectiveCompleted(15)
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0300_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && LC184_ResearchWingPassword != None
        playerRef.AddItem(LC184_ResearchWingPassword, 1, True)
    EndIf
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0400_Item_00()
    Alias_EnableMarker_Mercs.TryToEnable()
    Alias_Door_Mercenary.TryToEnable()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(35)
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0500_Item_00()
    Alias_EnableMarker_Mercs.TryToDisable()
    ObjectReference mercenaryDoor = Alias_Door_Mercenary.GetReference()
    If mercenaryDoor != None
        mercenaryDoor.Lock(False)
        mercenaryDoor.SetOpen()
    EndIf
    Alias_Actor_Rahmani_WestTek.TryToMoveTo(Alias_Marker_RahmaniArrival.GetReference())
    Alias_Actor_Shin_WestTek.TryToMoveTo(Alias_Marker_ShinArrival.GetReference())
    Alias_Actor_Blackburn_WestTek.TryToMoveTo(Alias_Marker_BlackburnArrival.GetReference())
    If BS02_MQ05_Catalyst_BrotherhoodArrival != None && !BS02_MQ05_Catalyst_BrotherhoodArrival.IsPlaying()
        BS02_MQ05_Catalyst_BrotherhoodArrival.Start()
    EndIf
    SetObjectiveCompleted(35)
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0600_Item_00()
    Alias_Actor_Rahmani_WestTek.TryToMoveTo(Alias_Marker_RahmaniMeetScientists.GetReference())
    Alias_Actor_Shin_WestTek.TryToMoveTo(Alias_Marker_ShinMeetScientists.GetReference())
    Alias_Actor_Blackburn_WestTek.TryToMoveTo(Alias_Marker_BlackburnMeetScientists.GetReference())
    Alias_Actor_Rahmani_WestTek.TryToEvaluatePackage()
    Alias_Actor_Shin_WestTek.TryToEvaluatePackage()
    Alias_Actor_Blackburn_WestTek.TryToEvaluatePackage()
    Alias_Actor_Farha_WestTek.TryToEvaluatePackage()
    Alias_Actor_Anthony_WestTek.TryToEvaluatePackage()
    Alias_Actor_Nellie_WestTek.TryToEvaluatePackage()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0700_Item_00()
    If BS02_MQ05_Catalyst_BlackburnTravelToBetrayal != None && !BS02_MQ05_Catalyst_BlackburnTravelToBetrayal.IsPlaying()
        BS02_MQ05_Catalyst_BlackburnTravelToBetrayal.Start()
    EndIf
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(70)
EndFunction

Function Fragment_Stage_0750_Item_00()
    If BS02_MQ05_Catalyst_BlackburnTravelToBetrayal != None && BS02_MQ05_Catalyst_BlackburnTravelToBetrayal.IsPlaying()
        BS02_MQ05_Catalyst_BlackburnTravelToBetrayal.Stop()
    EndIf
    If BS02_MQ05_Catalyst_BlackburnBetrayal != None && !BS02_MQ05_Catalyst_BlackburnBetrayal.IsPlaying()
        BS02_MQ05_Catalyst_BlackburnBetrayal.Start()
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    ObjectReference hatchDoor = Alias_Door_BossRoomHatch.GetReference()
    If hatchDoor != None
        hatchDoor.Lock(False)
        hatchDoor.SetOpen()
    EndIf
    Alias_Trigger_HatchDrop.TryToEnable()
    SetObjectiveCompleted(70)
    SetObjectiveDisplayed(80)
EndFunction

Function Fragment_Stage_0900_Item_00()
    Alias_Trigger_HatchDrop.TryToDisable()
    Alias_Door_BossRoomHatch.TryToDisable()
    If BS02_MQ05_Catalyst_TheExperiment != None && !BS02_MQ05_Catalyst_TheExperiment.IsPlaying()
        BS02_MQ05_Catalyst_TheExperiment.Start()
    EndIf
    SetObjectiveCompleted(80)
    SetObjectiveDisplayed(90)
EndFunction

Function Fragment_Stage_0950_Item_00()
    ObjectReference tubeRef = Alias_Static_Tube.GetReference()
    If tubeRef != None
        If TubeExplosion != None
            tubeRef.PlaceAtMe(TubeExplosion)
        EndIf
        tubeRef.Disable()
    EndIf
EndFunction

Function Fragment_Stage_0975_Item_00()
    Alias_Activator_TestChamberShield.TryToEnable()
EndFunction

Function Fragment_Stage_1000_Item_00()
    Alias_Actor_Behemoth.TryToMoveTo(Alias_Marker_BossRespawn.GetReference())
    Alias_Actor_Behemoth.TryToEnable()
    Actor bossRef = Alias_Actor_Behemoth.GetActorReference()
    Actor playerRef = Alias_Player.GetActorReference()
    If bossRef != None
        If BS02_MQ05_Catalyst_BrotherhoodEnemyFaction != None
            bossRef.AddToFaction(BS02_MQ05_Catalyst_BrotherhoodEnemyFaction)
        EndIf
        If PlayerEnemyFaction != None
            bossRef.AddToFaction(PlayerEnemyFaction)
        EndIf
        If playerRef != None
            bossRef.StartCombat(playerRef, True)
        EndIf
    EndIf
    If BS02_MQ05_Catalyst_BossHazards != None && !BS02_MQ05_Catalyst_BossHazards.IsPlaying()
        BS02_MQ05_Catalyst_BossHazards.Start()
    EndIf
    SetObjectiveCompleted(90)
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_1100_Item_00()
    If BS02_MQ05_Catalyst_BossHazards != None && BS02_MQ05_Catalyst_BossHazards.IsPlaying()
        BS02_MQ05_Catalyst_BossHazards.Stop()
    EndIf
    If BS02_MQ05_Catalyst_TheExperiment != None && BS02_MQ05_Catalyst_TheExperiment.IsPlaying()
        BS02_MQ05_Catalyst_TheExperiment.Stop()
    EndIf
    Alias_Activator_TestChamberShield.TryToDisable()
    Alias_Actor_Rahmani_WestTek.TryToMoveTo(Alias_Marker_RahmaniIntercom.GetReference())
    Alias_Actor_Shin_WestTek.TryToMoveTo(Alias_Marker_ShinIntercom.GetReference())
    If BS02_MQ05_Catalyst_AfterBattle != None && !BS02_MQ05_Catalyst_AfterBattle.IsPlaying()
        BS02_MQ05_Catalyst_AfterBattle.Start()
    EndIf
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(110)
EndFunction

Function Fragment_Stage_1175_Item_00()
    If BS02_MQ05_Catalyst_AfterBattle != None && BS02_MQ05_Catalyst_AfterBattle.IsPlaying()
        BS02_MQ05_Catalyst_AfterBattle.Stop()
    EndIf
    If BS02_MQ05_Catalyst_PostIntercom != None && !BS02_MQ05_Catalyst_PostIntercom.IsPlaying()
        BS02_MQ05_Catalyst_PostIntercom.Start()
    EndIf
EndFunction

Function Fragment_Stage_1210_Item_00()
    ObjectReference viewingDoor = Alias_Door_ViewingRoom.GetReference()
    If viewingDoor != None
        viewingDoor.Lock(False)
        viewingDoor.SetOpen()
    EndIf
    SetObjectiveCompleted(110)
    SetStage(1300)
EndFunction

Function Fragment_Stage_1220_Item_00()
    ObjectReference viewingDoor = Alias_Door_ViewingRoom.GetReference()
    If viewingDoor != None
        viewingDoor.Lock(False)
        viewingDoor.SetOpen()
    EndIf
    SetObjectiveCompleted(110)
    SetStage(1300)
EndFunction

Function Fragment_Stage_1230_Item_00()
    ObjectReference viewingDoor = Alias_Door_ViewingRoom.GetReference()
    If viewingDoor != None
        viewingDoor.Lock(False)
        viewingDoor.SetOpen()
    EndIf
    SetObjectiveCompleted(110)
    SetStage(1300)
EndFunction

Function Fragment_Stage_1240_Item_00()
    Alias_Terminal_ViewingRoom.TryToEnable()
    Alias_Key_ViewingRoomTerminalPassword.TryToEnable()
    SetObjectiveDisplayed(111)
    SetObjectiveDisplayed(112)
EndFunction

Function Fragment_Stage_1250_Item_00()
    ObjectReference viewingDoor = Alias_Door_ViewingRoom.GetReference()
    If viewingDoor != None
        viewingDoor.Lock(False)
        viewingDoor.SetOpen()
    EndIf
    SetObjectiveCompleted(111)
    SetStage(1300)
EndFunction

Function Fragment_Stage_1260_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    ObjectReference passwordRef = Alias_Key_ViewingRoomTerminalPassword.GetReference()
    If playerRef != None && passwordRef != None
        playerRef.AddItem(passwordRef, 1, True)
    EndIf
    SetObjectiveCompleted(112)
EndFunction

Function Fragment_Stage_1300_Item_00()
    SetObjectiveCompleted(110)
    If IsObjectiveDisplayed(111)
        SetObjectiveCompleted(111)
    EndIf
    If IsObjectiveDisplayed(112) && !IsObjectiveCompleted(112)
        SetObjectiveDisplayed(112, False)
    EndIf
    Alias_Door_ScientistExit.TryToEnable()
    SetObjectiveDisplayed(120)
EndFunction

Function Fragment_Stage_1310_Item_00()
    SetObjectiveDisplayed(126)
    SetObjectiveDisplayed(127)
EndFunction

Function Fragment_Stage_1311_Item_00()
    If IsStageDone(1313) && !IsStageDone(1316)
        SetStage(1316)
    EndIf
EndFunction

Function Fragment_Stage_1313_Item_00()
    If IsStageDone(1311) && !IsStageDone(1316)
        SetStage(1316)
    EndIf
EndFunction

Function Fragment_Stage_1314_Item_00()
    If IsStageDone(1315) && !IsStageDone(1317)
        SetStage(1317)
    EndIf
EndFunction

Function Fragment_Stage_1315_Item_00()
    If IsStageDone(1314) && !IsStageDone(1317)
        SetStage(1317)
    EndIf
EndFunction

Function Fragment_Stage_1316_Item_00()
    SetObjectiveCompleted(126)
EndFunction

Function Fragment_Stage_1317_Item_00()
    SetObjectiveCompleted(127)
EndFunction

Function Fragment_Stage_1330_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        playerRef.SetValue(BS02_AV_BrotherhoodLeaderFinal, 1.0)
    EndIf
    Alias_Actor_Rahmani_WestTek.TryToMoveTo(Alias_Marker_RahmaniChoice.GetReference())
    Alias_Actor_Shin_WestTek.TryToMoveTo(Alias_Marker_ShinChoice.GetReference())
    SetObjectiveCompleted(120)
    SetObjectiveCompleted(125)
    SetObjectiveDisplayed(129)
    If !IsStageDone(1400)
        SetStage(1400)
    EndIf
EndFunction

Function Fragment_Stage_1340_Item_00()
    Actor shinRef = Alias_Actor_Shin_WestTek.GetActorReference()
    Actor playerRef = Alias_Player.GetActorReference()
    If shinRef != None
        shinRef.SetProtected(False)
        If BrotherhoodofSteelFaction != None
            shinRef.RemoveFromFaction(BrotherhoodofSteelFaction)
        EndIf
        If PlayerAllyFaction != None
            shinRef.RemoveFromFaction(PlayerAllyFaction)
        EndIf
        If BS02_MQ05_Catalyst_BrotherhoodEnemyFaction != None
            shinRef.AddToFaction(BS02_MQ05_Catalyst_BrotherhoodEnemyFaction)
        EndIf
        If PlayerEnemyFaction != None
            shinRef.AddToFaction(PlayerEnemyFaction)
        EndIf
        If playerRef != None
            shinRef.StartCombat(playerRef, True)
        EndIf
    EndIf
    SetObjectiveCompleted(129)
    SetObjectiveDisplayed(121)
EndFunction

Function Fragment_Stage_1360_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        playerRef.SetValue(BS02_AV_BrotherhoodLeaderFinal, 2.0)
    EndIf
    Alias_Actor_Rahmani_WestTek.TryToMoveTo(Alias_Marker_RahmaniChoice.GetReference())
    Alias_Actor_Shin_WestTek.TryToMoveTo(Alias_Marker_ShinChoice.GetReference())
    SetObjectiveCompleted(120)
    SetObjectiveCompleted(125)
    SetObjectiveDisplayed(128)
    If !IsStageDone(1400)
        SetStage(1400)
    EndIf
EndFunction

Function Fragment_Stage_1370_Item_00()
    Actor rahmaniRef = Alias_Actor_Rahmani_WestTek.GetActorReference()
    Actor playerRef = Alias_Player.GetActorReference()
    If rahmaniRef != None
        rahmaniRef.SetProtected(False)
        If BrotherhoodofSteelFaction != None
            rahmaniRef.RemoveFromFaction(BrotherhoodofSteelFaction)
        EndIf
        If PlayerAllyFaction != None
            rahmaniRef.RemoveFromFaction(PlayerAllyFaction)
        EndIf
        If BS02_MQ05_Catalyst_BrotherhoodEnemyFaction != None
            rahmaniRef.AddToFaction(BS02_MQ05_Catalyst_BrotherhoodEnemyFaction)
        EndIf
        If PlayerEnemyFaction != None
            rahmaniRef.AddToFaction(PlayerEnemyFaction)
        EndIf
        If playerRef != None
            rahmaniRef.StartCombat(playerRef, True)
        EndIf
    EndIf
    SetObjectiveCompleted(128)
    SetObjectiveDisplayed(122)
EndFunction

Function Fragment_Stage_1401_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        playerRef.SetValue(BS01_RahmaniAwayValue, 1.0)
    EndIf
    Alias_Actor_Rahmani_WestTek.TryToMoveTo(Alias_Marker_Banishment.GetReference())
    Alias_Actor_Rahmani_WestTek.TryToEvaluatePackage()
    SetObjectiveCompleted(128)
EndFunction

Function Fragment_Stage_1402_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        playerRef.SetValue(BS01_ShinAwayValue, 1.0)
    EndIf
    Alias_Actor_Shin_WestTek.TryToMoveTo(Alias_Marker_Banishment.GetReference())
    Alias_Actor_Shin_WestTek.TryToEvaluatePackage()
    SetObjectiveCompleted(129)
EndFunction

Function Fragment_Stage_1405_Item_00()
    If BS02_MQ05_Catalyst_KillingScientists != None && !BS02_MQ05_Catalyst_KillingScientists.IsPlaying()
        BS02_MQ05_Catalyst_KillingScientists.Start()
    EndIf
EndFunction

Function Fragment_Stage_1410_Item_00()
    Actor farhaRef = Alias_Actor_Farha_WestTek.GetActorReference()
    Actor anthonyRef = Alias_Actor_Anthony_WestTek.GetActorReference()
    Actor nellieRef = Alias_Actor_Nellie_WestTek.GetActorReference()
    If farhaRef != None
        farhaRef.SetProtected(False)
    EndIf
    If anthonyRef != None
        anthonyRef.SetProtected(False)
    EndIf
    If nellieRef != None
        nellieRef.SetProtected(False)
    EndIf
EndFunction

Function Fragment_Stage_1411_Item_00()
    Alias_Actor_Farha_WestTek.TryToKill()
EndFunction

Function Fragment_Stage_1412_Item_00()
    Alias_Actor_Anthony_WestTek.TryToKill()
EndFunction

Function Fragment_Stage_1413_Item_00()
    Alias_Actor_Nellie_WestTek.TryToKill()
EndFunction

Function Fragment_Stage_1450_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        If IsStageDone(1340)
            playerRef.SetValue(BS02_AV_BrotherhoodLeaderDead, 2.0)
            playerRef.SetValue(BS01_ShinAwayValue, 1.0)
            SetObjectiveCompleted(121)
        ElseIf IsStageDone(1370)
            playerRef.SetValue(BS02_AV_BrotherhoodLeaderDead, 1.0)
            playerRef.SetValue(BS01_RahmaniAwayValue, 1.0)
            SetObjectiveCompleted(122)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1460_Item_00()
    If BS02_MQ05_Catalyst_KillingScientists != None && BS02_MQ05_Catalyst_KillingScientists.IsPlaying()
        BS02_MQ05_Catalyst_KillingScientists.Stop()
    EndIf
    If BS02_MQ05_Catalyst_PostKilledScientists != None && !BS02_MQ05_Catalyst_PostKilledScientists.IsPlaying()
        BS02_MQ05_Catalyst_PostKilledScientists.Start()
    EndIf
    SetObjectiveCompleted(125)
EndFunction

Function Fragment_Stage_1475_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        Float finalLeader = playerRef.GetValue(BS02_AV_BrotherhoodLeaderFinal)
        If finalLeader == 1.0
            playerRef.SetValue(BS01_RahmaniAwayValue, 0.0)
            playerRef.SetValue(BS01_ShinAwayValue, 1.0)
        ElseIf finalLeader == 2.0
            playerRef.SetValue(BS01_RahmaniAwayValue, 1.0)
            playerRef.SetValue(BS01_ShinAwayValue, 0.0)
        EndIf
    EndIf
    If IsObjectiveDisplayed(120)
        SetObjectiveCompleted(120)
    EndIf
    If IsObjectiveDisplayed(121)
        SetObjectiveCompleted(121)
    EndIf
    If IsObjectiveDisplayed(122)
        SetObjectiveCompleted(122)
    EndIf
    If IsObjectiveDisplayed(125)
        SetObjectiveCompleted(125)
    EndIf
    If IsObjectiveDisplayed(126) && !IsObjectiveCompleted(126)
        SetObjectiveDisplayed(126, False)
    EndIf
    If IsObjectiveDisplayed(127) && !IsObjectiveCompleted(127)
        SetObjectiveDisplayed(127, False)
    EndIf
    Alias_Door_ScientistExit.TryToEnable()
    ObjectReference scientistExit = Alias_Door_ScientistExit.GetReference()
    If scientistExit != None
        scientistExit.Lock(False)
        scientistExit.SetOpen()
    EndIf
    SetObjectiveDisplayed(130)
EndFunction

Function Fragment_Stage_1500_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    Float finalLeader = 0.0
    If playerRef != None
        finalLeader = playerRef.GetValue(BS02_AV_BrotherhoodLeaderFinal)
    EndIf
    Alias_EnableMarker_FortAtlasInitiates.TryToEnable()
    SetAmbientStoryManagerMarkersEnabled(True)
    Alias_Actor_Valdez.TryToMoveTo(Alias_Marker_ValdezAddress.GetReference())
    Alias_Actor_Dorsey.TryToMoveTo(Alias_Marker_DorseyAddress.GetReference())
    Alias_Actor_Hewsen.TryToMoveTo(Alias_Marker_HewsenAddress.GetReference())
    Alias_Actor_Marcia.TryToMoveTo(Alias_Marker_MarciaAddress.GetReference())
    Alias_Actor_Max.TryToMoveTo(Alias_Marker_MaxAddress.GetReference())
    Alias_Actor_MartyPutnam.TryToMoveTo(Alias_Marker_PutnamAddress.GetReference())
    Alias_Actor_ColinPutnam.TryToMoveTo(Alias_Marker_PutnamAddress.GetReference())
    Alias_actor_Dodge.TryToMoveTo(Alias_Marker_DodgeAddress.GetReference())
    Alias_Actor_Ramirez.TryToMoveTo(Alias_Marker_RamirezAddress.GetReference())
    If finalLeader == 1.0
        Alias_Actor_Rahmani_FortAtlas.TryToEnable()
        Alias_Actor_Rahmani_FortAtlas.TryToMoveTo(Alias_Marker_LeaderAddress.GetReference())
        Alias_Actor_Shin_FortAtlas.TryToDisable()
        Alias_Actor_Farha_FortAtlas.TryToEnable()
        Alias_Actor_Farha_FortAtlas.TryToMoveTo(Alias_Marker_FarhaAddress.GetReference())
        Alias_Actor_Nellie_FortAtlas.TryToEnable()
        Alias_Actor_Nellie_FortAtlas.TryToMoveTo(Alias_Marker_NellieAddress.GetReference())
        Alias_Actor_Jain_FortAtlas.TryToEnable()
        Alias_Actor_Jain_FortAtlas.TryToMoveTo(Alias_Marker_JainAddress.GetReference())
        SetObjectiveDisplayed(140)
    ElseIf finalLeader == 2.0
        Alias_Actor_Shin_FortAtlas.TryToEnable()
        Alias_Actor_Shin_FortAtlas.TryToMoveTo(Alias_Marker_LeaderAddress.GetReference())
        Alias_Actor_Rahmani_FortAtlas.TryToDisable()
        Alias_Actor_Farha_FortAtlas.TryToDisable()
        Alias_Actor_Nellie_FortAtlas.TryToDisable()
        Alias_Actor_Jain_FortAtlas.TryToDisable()
        SetObjectiveDisplayed(145)
    EndIf
    SetObjectiveCompleted(130)
EndFunction

Function Fragment_Stage_1550_Item_00()
    If BS02_MQ05_Catalyst_FortAtlasAddress != None && !BS02_MQ05_Catalyst_FortAtlasAddress.IsPlaying()
        BS02_MQ05_Catalyst_FortAtlasAddress.Start()
    EndIf
EndFunction

Function Fragment_Stage_1600_Item_00()
    If IsObjectiveDisplayed(140)
        SetObjectiveCompleted(140)
    EndIf
    If IsObjectiveDisplayed(145)
        SetObjectiveCompleted(145)
    EndIf
    Alias_Trigger_KnightErrant.TryToEnable()
    SetObjectiveDisplayed(150)
EndFunction

Function Fragment_Stage_1601_Item_00()
    Int i = 0
    While i < Alias_Actors_AddressInitiates.GetCount()
        Actor initiateRef = Alias_Actors_AddressInitiates.GetAt(i) As Actor
        If initiateRef != None && IdleCheeringStanding != None
            initiateRef.PlayIdle(IdleCheeringStanding)
        EndIf
        i += 2
    EndWhile
EndFunction

Function Fragment_Stage_1602_Item_00()
    Int i = 1
    While i < Alias_Actors_AddressInitiates.GetCount()
        Actor initiateRef = Alias_Actors_AddressInitiates.GetAt(i) As Actor
        If initiateRef != None && IdleCheeringStanding != None
            initiateRef.PlayIdle(IdleCheeringStanding)
        EndIf
        i += 2
    EndWhile
EndFunction

Function Fragment_Stage_1650_Item_00()
    Alias_Trigger_KnightErrant.TryToDisable()
    If BS02_MQ05_Catalyst_KnightErrant != None && !BS02_MQ05_Catalyst_KnightErrant.IsPlaying()
        BS02_MQ05_Catalyst_KnightErrant.Start()
    EndIf
EndFunction

Function Fragment_Stage_1700_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    Float finalLeader = 0.0
    If playerRef != None
        finalLeader = playerRef.GetValue(BS02_AV_BrotherhoodLeaderFinal)
    EndIf
    If finalLeader == 1.0
        SetObjectiveDisplayed(160)
    ElseIf finalLeader == 2.0
        SetObjectiveDisplayed(165)
    EndIf
    SetObjectiveCompleted(150)
    SetObjectiveDisplayed(170)
    SetObjectiveDisplayed(171)
EndFunction

Function Fragment_Stage_1710_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        playerRef.SetValue(BS02_MQ05_Catalyst_DorseyAwayValue, 1.0)
    EndIf
    Alias_Actor_Dorsey.TryToMoveTo(Alias_Marker_DorseyAddress.GetReference())
    Alias_Actor_Dorsey.TryToEvaluatePackage()
    If BS02_MQ05_Catalyst_DorseyExit != None && !BS02_MQ05_Catalyst_DorseyExit.IsPlaying()
        BS02_MQ05_Catalyst_DorseyExit.Start()
    EndIf
    If IsStageDone(1720)
        SetObjectiveCompleted(171)
    EndIf
EndFunction

Function Fragment_Stage_1720_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        playerRef.SetValue(BS02_HewsenAwayValue, 1.0)
    EndIf
    Alias_Actor_Hewsen.TryToEvaluatePackage()
    If IsStageDone(1710)
        SetObjectiveCompleted(171)
    EndIf
EndFunction

Function Fragment_Stage_1760_Item_00()
    SetObjectiveCompleted(170)
EndFunction

Function Fragment_Stage_9000_Item_00()
    If IsObjectiveDisplayed(160)
        SetObjectiveCompleted(160)
    EndIf
    If IsObjectiveDisplayed(165)
        SetObjectiveCompleted(165)
    EndIf
    If IsObjectiveDisplayed(170)
        SetObjectiveCompleted(170)
    EndIf
    If IsObjectiveDisplayed(171)
        SetObjectiveCompleted(171)
    EndIf
    CompleteQuest()
EndFunction

Function Fragment_Stage_10000_Item_00()
    If BS02_MQ05_Catalyst_PreIntro != None && BS02_MQ05_Catalyst_PreIntro.IsPlaying()
        BS02_MQ05_Catalyst_PreIntro.Stop()
    EndIf
    If BS02_MQ05_Catalyst_BrotherhoodArrival != None && BS02_MQ05_Catalyst_BrotherhoodArrival.IsPlaying()
        BS02_MQ05_Catalyst_BrotherhoodArrival.Stop()
    EndIf
    If BS02_MQ05_Catalyst_BlackburnTravelToBetrayal != None && BS02_MQ05_Catalyst_BlackburnTravelToBetrayal.IsPlaying()
        BS02_MQ05_Catalyst_BlackburnTravelToBetrayal.Stop()
    EndIf
    If BS02_MQ05_Catalyst_BlackburnBetrayal != None && BS02_MQ05_Catalyst_BlackburnBetrayal.IsPlaying()
        BS02_MQ05_Catalyst_BlackburnBetrayal.Stop()
    EndIf
    If BS02_MQ05_Catalyst_TheExperiment != None && BS02_MQ05_Catalyst_TheExperiment.IsPlaying()
        BS02_MQ05_Catalyst_TheExperiment.Stop()
    EndIf
    If BS02_MQ05_Catalyst_BossHazards != None && BS02_MQ05_Catalyst_BossHazards.IsPlaying()
        BS02_MQ05_Catalyst_BossHazards.Stop()
    EndIf
    If BS02_MQ05_Catalyst_AfterBattle != None && BS02_MQ05_Catalyst_AfterBattle.IsPlaying()
        BS02_MQ05_Catalyst_AfterBattle.Stop()
    EndIf
    If BS02_MQ05_Catalyst_PostIntercom != None && BS02_MQ05_Catalyst_PostIntercom.IsPlaying()
        BS02_MQ05_Catalyst_PostIntercom.Stop()
    EndIf
    If BS02_MQ05_Catalyst_KillingScientists != None && BS02_MQ05_Catalyst_KillingScientists.IsPlaying()
        BS02_MQ05_Catalyst_KillingScientists.Stop()
    EndIf
    If BS02_MQ05_Catalyst_PostKilledScientists != None && BS02_MQ05_Catalyst_PostKilledScientists.IsPlaying()
        BS02_MQ05_Catalyst_PostKilledScientists.Stop()
    EndIf
    If BS02_MQ05_Catalyst_FortAtlasAddress != None && BS02_MQ05_Catalyst_FortAtlasAddress.IsPlaying()
        BS02_MQ05_Catalyst_FortAtlasAddress.Stop()
    EndIf
    If BS02_MQ05_Catalyst_KnightErrant != None && BS02_MQ05_Catalyst_KnightErrant.IsPlaying()
        BS02_MQ05_Catalyst_KnightErrant.Stop()
    EndIf
    Alias_EnableMarker_Mercs.TryToDisable()
    Alias_Activator_TestChamberShield.TryToDisable()
    Alias_Trigger_HatchDrop.TryToDisable()
    Alias_Actor_Behemoth.TryToDisable()
    Int i = 0
    While i < Alias_Actors_Mercs.GetCount()
        ObjectReference mercRef = Alias_Actors_Mercs.GetAt(i)
        If mercRef != None
            mercRef.Disable()
        EndIf
        i += 1
    EndWhile
    Stop()
EndFunction
