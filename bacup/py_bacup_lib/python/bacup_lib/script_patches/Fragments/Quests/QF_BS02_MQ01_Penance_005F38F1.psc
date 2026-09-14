Function Fragment_Stage_0001_Item_00()
    If !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0002_Item_00()
    If !IsStageDone(900)
        SetStage(900)
    EndIf
EndFunction

Function Fragment_Stage_0003_Item_00()
    If !IsStageDone(1400)
        SetStage(1400)
    EndIf
EndFunction

Function Fragment_Stage_0004_Item_00()
    If !IsStageDone(1800)
        SetStage(1800)
    EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
    Actor rahmani = Alias_Actor_Rahmani_FortAtlas.GetActorReference()
    Actor shin = Alias_Actor_Shin_FortAtlas.GetActorReference()
    Actor hewsen = Alias_Actor_Hewsen_FortAtlas.GetActorReference()
    Actor max = Alias_Actor_Max.GetActorReference()
    ObjectReference rahmaniMarker = Alias_Marker_RahmaniArmory.GetReference()
    ObjectReference shinMarker = Alias_Marker_ShinArmory.GetReference()
    ObjectReference hewsenMarker = Alias_Marker_HewsenCafe.GetReference()
    ObjectReference maxMarker = Alias_Marker_MaxCafe.GetReference()
    If rahmani != None && rahmaniMarker != None
        rahmani.MoveTo(rahmaniMarker)
    EndIf
    If shin != None && shinMarker != None
        shin.MoveTo(shinMarker)
    EndIf
    If hewsen != None && hewsenMarker != None
        hewsen.MoveTo(hewsenMarker)
    EndIf
    If max != None && maxMarker != None
        max.MoveTo(maxMarker)
    EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
    Actor player = Alias_Player.GetActorReference()
    Actor shin = Alias_Actor_Shin_Caverns.GetActorReference()
    Actor hewsen = Alias_Actor_Hewsen_Caverns.GetActorReference()
    Actor norland = Alias_Actor_Norland_Caverns.GetActorReference()
    If player != None
        player.SetValue(BS01_ShinAwayValue, 1.0)
        player.SetValue(BS02_HewsenAwayValue, 1.0)
        player.SetValue(BS02_NorlandAwayValue, 1.0)
        player.SetValue(BS02_MarciaAwayValue, 1.0)
        player.SetValue(BS02_ArtKnappAwayValue, 1.0)
    EndIf
    If shin != None
        shin.Enable()
        shin.EvaluatePackage()
    EndIf
    If hewsen != None
        hewsen.Enable()
        hewsen.EvaluatePackage()
    EndIf
    If norland != None
        norland.Enable()
        norland.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
    Actor shin = Alias_Actor_Shin_CavernsInt.GetActorReference()
    Actor hewsen = Alias_Actor_Hewsen_CavernsInt.GetActorReference()
    If Alias_Actors_ToDisable != None
        Alias_Actors_ToDisable.DisableAll()
    EndIf
    If shin != None
        shin.Enable()
        shin.EvaluatePackage()
    EndIf
    If hewsen != None
        hewsen.Enable()
        hewsen.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0210_Item_00()
    If BS02_MQ01_Penance_RahmaniShinAmbient != None
        BS02_MQ01_Penance_RahmaniShinAmbient.Start()
    EndIf
EndFunction

Function Fragment_Stage_0220_Item_00()
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0225_Item_00()
    Actor rahmani = Alias_Actor_Rahmani_FortAtlas.GetActorReference()
    Actor shin = Alias_Actor_Shin_FortAtlas.GetActorReference()
    If rahmani != None
        rahmani.EvaluatePackage()
    EndIf
    If shin != None
        shin.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0227_Item_00()
    Actor rahmani = Alias_Actor_Rahmani_FortAtlas.GetActorReference()
    If rahmani != None
        rahmani.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0228_Item_00()
    Actor shin = Alias_Actor_Shin_FortAtlas.GetActorReference()
    If shin != None
        shin.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0230_Item_00()
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0310_Item_00()
    If BS02_MQ01_Penance_HewsenMaxAmbient != None
        BS02_MQ01_Penance_HewsenMaxAmbient.Start()
    EndIf
EndFunction

Function Fragment_Stage_0320_Item_00()
    SetObjectiveDisplayed(30)
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0326_Item_00()
    Actor hewsen = Alias_Actor_Hewsen_FortAtlas.GetActorReference()
    If hewsen != None && AnimFaceArchetypeWorried != None
        hewsen.ChangeAnimFaceArchetype(AnimFaceArchetypeWorried)
    EndIf
EndFunction

Function Fragment_Stage_0327_Item_00()
    Actor hewsen = Alias_Actor_Hewsen_FortAtlas.GetActorReference()
    If hewsen != None
        hewsen.ChangeAnimFaceArchetype()
    EndIf
EndFunction

Function Fragment_Stage_0330_Item_00()
    Actor hewsen = Alias_Actor_Hewsen_FortAtlas.GetActorReference()
    If hewsen != None && BS01_AV_IsInitiate != None
        hewsen.SetValue(BS01_AV_IsInitiate, 1.0)
    EndIf
    SetObjectiveCompleted(30)
EndFunction

Function Fragment_Stage_0340_Item_00()
    Actor norland = Alias_Actor_Norland_Caverns.GetActorReference()
    If norland != None && BS01_AV_IsInitiate != None
        norland.SetValue(BS01_AV_IsInitiate, 1.0)
    EndIf
    SetObjectiveCompleted(40)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0450_Item_00()
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0500_Item_00()
    ObjectReference enableMarker = Alias_EnableMarker_Mirelurks.GetReference()
    Actor mirelurk = Alias_Actor_MirelurkPhotoOp.GetActorReference()
    If enableMarker != None
        enableMarker.Enable()
    EndIf
    If mirelurk != None
        mirelurk.Enable()
        mirelurk.EvaluatePackage()
    EndIf
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0600_Item_00()
    If BS02_MQ01_Penance_ApproachPhotoOp != None
        BS02_MQ01_Penance_ApproachPhotoOp.Start()
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    ObjectReference enableMarker = Alias_EnableMarker_Floaters.GetReference()
    If enableMarker != None
        enableMarker.Enable()
    EndIf
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(65)
EndFunction

Function Fragment_Stage_0750_Item_00()
    If BS02_MQ01_Penance_PhotoOpCameraReaction != None
        BS02_MQ01_Penance_PhotoOpCameraReaction.Start()
    EndIf
EndFunction

Function Fragment_Stage_0770_Item_00()
    Actor shin = Alias_Actor_Shin_Caverns.GetActorReference()
    Actor hewsen = Alias_Actor_Hewsen_Caverns.GetActorReference()
    Actor norland = Alias_Actor_Norland_Caverns.GetActorReference()
    If shin != None
        shin.EvaluatePackage()
    EndIf
    If hewsen != None
        hewsen.EvaluatePackage()
    EndIf
    If norland != None
        norland.EvaluatePackage()
    EndIf
    SetObjectiveCompleted(65)
    SetObjectiveDisplayed(68)
EndFunction

Function Fragment_Stage_0780_Item_00()
    Actor shin = Alias_Actor_Shin_Caverns.GetActorReference()
    If shin != None
        shin.EvaluatePackage()
    EndIf
    SetObjectiveCompleted(68)
    SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0800_Item_00()
    If BS02_MQ01_Penance_ApproachDeadEnd != None
        BS02_MQ01_Penance_ApproachDeadEnd.Start()
    EndIf
EndFunction

Function Fragment_Stage_0850_Item_00()
    If BS02_MQ01_Penance_ShinAfterCombat != None
        BS02_MQ01_Penance_ShinAfterCombat.Start()
    EndIf
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(65)
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(65)
    SetObjectiveDisplayed(70)
EndFunction

Function Fragment_Stage_0995_Item_00()
    If BS02_MQ01_Penance_HewsenRockslideRemark != None
        BS02_MQ01_Penance_HewsenRockslideRemark.Start()
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    If IsStageDone(1010) && !IsStageDone(1200)
        SetStage(1200)
    EndIf
EndFunction

Function Fragment_Stage_1005_Item_00()
    If BS02_MQ01_Penance_NorlandCreviceRemark != None
        BS02_MQ01_Penance_NorlandCreviceRemark.Start()
    EndIf
EndFunction

Function Fragment_Stage_1010_Item_00()
    If IsStageDone(1000) && !IsStageDone(1200)
        SetStage(1200)
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveCompleted(70)
    SetObjectiveDisplayed(80)
EndFunction

Function Fragment_Stage_1250_Item_00()
    If IsStageDone(1260) && !IsStageDone(1270)
        SetStage(1270)
    EndIf
EndFunction

Function Fragment_Stage_1260_Item_00()
    If IsStageDone(1250) && !IsStageDone(1270)
        SetStage(1270)
    EndIf
EndFunction

Function Fragment_Stage_1270_Item_00()
    ObjectReference shovelRef = Alias_QO_Weap_Shovel.GetReference()
    ObjectReference shovelMarker = Alias_Marker_Shovel.GetReference()
    ObjectReference dynamiteRef = Alias_QO_Weap_Dynamite.GetReference()
    ObjectReference dynamiteContainer = Alias_Container_Dynamite.GetReference()
    If shovelRef == None && shovelMarker != None && Shovel != None
        shovelRef = shovelMarker.PlaceAtMe(Shovel)
        If shovelRef != None
            Alias_QO_Weap_Shovel.ForceRefTo(shovelRef)
        EndIf
    EndIf
    If dynamiteRef == None && dynamiteContainer != None && DynamiteBundleGrenade != None
        dynamiteRef = dynamiteContainer.PlaceAtMe(DynamiteBundleGrenade)
        If dynamiteRef != None
            Alias_QO_Weap_Dynamite.ForceRefTo(dynamiteRef)
            dynamiteContainer.AddItem(dynamiteRef, 1, True)
        EndIf
    EndIf
    If BS02_MQ01_Penance_InitiatesCrawlThroughCrevice != None
        BS02_MQ01_Penance_InitiatesCrawlThroughCrevice.Start()
    EndIf
    SetObjectiveCompleted(80)
    SetObjectiveDisplayed(90)
    SetObjectiveDisplayed(95)
EndFunction

Function Fragment_Stage_1280_Item_00()
    Actor hewsen = Alias_Actor_Hewsen_Caverns.GetActorReference()
    Actor norland = Alias_Actor_Norland_Caverns.GetActorReference()
    If hewsen != None
        hewsen.EvaluatePackage()
    EndIf
    If norland != None
        norland.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1304_Item_00()
    If BS02_MQ01_Penance_NorlandShovelRemark != None
        BS02_MQ01_Penance_NorlandShovelRemark.Start()
    EndIf
EndFunction

Function Fragment_Stage_1305_Item_00()
    If IsStageDone(1328) && !IsStageDone(1350)
        SetStage(1350)
    EndIf
EndFunction

Function Fragment_Stage_1310_Item_00()
    If !IsStageDone(1400)
        SetStage(1400)
    EndIf
EndFunction

Function Fragment_Stage_1315_Item_00()
    Int response = -1
    If BS02_MQ01_Penance_RockslideMessageStrengthFail != None
        response = BS02_MQ01_Penance_RockslideMessageStrengthFail.Show()
    EndIf
    If response == 0 && BS02_MQ01_Penance_RockslideMessageStrengthFailCry != None
        BS02_MQ01_Penance_RockslideMessageStrengthFailCry.Show()
    EndIf
EndFunction

Function Fragment_Stage_1320_Item_00()
    If !IsStageDone(1400)
        SetStage(1400)
    EndIf
EndFunction

Function Fragment_Stage_1325_Item_00()
    If BS02_MQ01_Penance_RockslideMessageLuckFail != None
        BS02_MQ01_Penance_RockslideMessageLuckFail.Show()
    EndIf
EndFunction

Function Fragment_Stage_1327_Item_00()
    ObjectReference dynamiteRef = Alias_QO_Weap_Dynamite.GetReference()
    ObjectReference dynamiteContainer = Alias_Container_Dynamite.GetReference()
    If dynamiteRef == None && dynamiteContainer != None && DynamiteBundleGrenade != None
        dynamiteRef = dynamiteContainer.PlaceAtMe(DynamiteBundleGrenade)
        If dynamiteRef != None
            Alias_QO_Weap_Dynamite.ForceRefTo(dynamiteRef)
            dynamiteContainer.AddItem(dynamiteRef, 1, True)
        EndIf
    EndIf
    If BS02_MQ01_Penance_HewsenDynamiteRemark != None
        BS02_MQ01_Penance_HewsenDynamiteRemark.Start()
    EndIf
EndFunction

Function Fragment_Stage_1328_Item_00()
    If IsStageDone(1305) && !IsStageDone(1350)
        SetStage(1350)
    EndIf
EndFunction

Function Fragment_Stage_1330_Item_00()
    ObjectReference player = Alias_Player.GetReference()
    ObjectReference dynamiteRef = Alias_QO_Weap_Dynamite.GetReference()
    If player != None
        If dynamiteRef != None && dynamiteRef.GetContainer() == player
            player.RemoveItem(dynamiteRef, 1, True)
        ElseIf DynamiteBundleGrenade != None && player.GetItemCount(DynamiteBundleGrenade) > 0
            player.RemoveItem(DynamiteBundleGrenade, 1, True)
        ElseIf BS02_MQ01_Penance_DynamiteBundleGrenade != None && player.GetItemCount(BS02_MQ01_Penance_DynamiteBundleGrenade) > 0
            player.RemoveItem(BS02_MQ01_Penance_DynamiteBundleGrenade, 1, True)
        EndIf
    EndIf
    If !IsStageDone(1400)
        SetStage(1400)
    EndIf
EndFunction

Function Fragment_Stage_1340_Item_00()
    SetObjectiveDisplayed(90)
EndFunction

Function Fragment_Stage_1350_Item_00()
    SetObjectiveCompleted(95)
    SetObjectiveDisplayed(90)
EndFunction

Function Fragment_Stage_1400_Item_00()
    ObjectReference rockslide = Alias_Activator_Rockslide.GetReference()
    ObjectReference navBlocker = Alias_Static_RockNavBlocker.GetReference()
    ObjectReference enableMarker = Alias_EnableMarker_Caverns.GetReference()
    ObjectReference spawnCenter = Alias_SpawnCenter_Wave1.GetReference()
    If rockslide != None
        If ExplosionVault79EntranceRocks != None
            rockslide.PlaceAtMe(ExplosionVault79EntranceRocks)
        EndIf
        rockslide.Disable()
    EndIf
    If navBlocker != None
        navBlocker.Disable()
    EndIf
    If enableMarker != None
        enableMarker.Enable()
    EndIf
    If spawnCenter != None
        spawnCenter.Enable()
        Quests:BS02_MQ01_Penance:QuestScript questScript = (Self as Quest) as Quests:BS02_MQ01_Penance:QuestScript
        If questScript != None
            questScript.StartLocalWave(spawnCenter, 1500)
        EndIf
    EndIf
    SetObjectiveCompleted(90)
    SetObjectiveCompleted(95)
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_1500_Item_00()
    If BS02_MQ01_Penance_PathCleared != None
        BS02_MQ01_Penance_PathCleared.Start()
    EndIf
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(110)
EndFunction

Function Fragment_Stage_1600_Item_00()
    If BS02_MQ01_Penance_ShinApproachCorpse != None
        BS02_MQ01_Penance_ShinApproachCorpse.Start()
    EndIf
    SetObjectiveCompleted(110)
    SetObjectiveDisplayed(120)
EndFunction

Function Fragment_Stage_1650_Item_00()
    ObjectReference corpse = Alias_Actor_NorlandCorpse.GetReference()
    Actor shin = Alias_Actor_Shin_Caverns.GetActorReference()
    ObjectReference tunnelMarker = Alias_Marker_ShinTunnel.GetReference()
    If corpse != None
        corpse.Enable()
    EndIf
    If shin != None && tunnelMarker != None
        shin.MoveTo(tunnelMarker)
        shin.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1700_Item_00()
    SetObjectiveCompleted(120)
    SetObjectiveDisplayed(130)
EndFunction

Function Fragment_Stage_1800_Item_00()
    SetObjectiveCompleted(130)
    SetObjectiveDisplayed(140)
EndFunction

Function Fragment_Stage_1850_Item_00()
    Actor shin = Alias_Actor_Shin_CavernsInt.GetActorReference()
    Actor hewsen = Alias_Actor_Hewsen_CavernsInt.GetActorReference()
    If Alias_Actors_ToDisable != None
        Alias_Actors_ToDisable.DisableAll()
    EndIf
    If shin != None
        shin.Enable()
        shin.EvaluatePackage()
    EndIf
    If hewsen != None
        hewsen.Enable()
        hewsen.EvaluatePackage()
    EndIf
    SetObjectiveCompleted(140)
    SetObjectiveDisplayed(190)
EndFunction

Function Fragment_Stage_1900_Item_00()
    SetObjectiveCompleted(190)
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_2000_Item_00()
    Actor shin = Alias_Actor_Shin_CavernsInt.GetActorReference()
    Actor hewsen = Alias_Actor_Hewsen_CavernsInt.GetActorReference()
    If shin != None && BS02_MQ01_Penance_NoIdleDialogueKeyword != None
        shin.AddKeyword(BS02_MQ01_Penance_NoIdleDialogueKeyword)
    EndIf
    If hewsen != None
        If BS02_MQ01_Penance_NoIdleDialogueKeyword != None
            hewsen.AddKeyword(BS02_MQ01_Penance_NoIdleDialogueKeyword)
        EndIf
        If AnimFaceArchetypeWorried != None
            hewsen.ChangeAnimFaceArchetype(AnimFaceArchetypeWorried)
        EndIf
    EndIf
    If BS02_MQ01_Penance_MineScene != None
        BS02_MQ01_Penance_MineScene.Start()
    EndIf
    SetObjectiveCompleted(200)
    SetObjectiveDisplayed(205)
EndFunction

Function Fragment_Stage_2100_Item_00()
    Actor shin = Alias_Actor_Shin_CavernsInt.GetActorReference()
    If shin != None && IdleMineExplosionReady != None
        shin.PlayIdle(IdleMineExplosionReady)
    EndIf
    SetObjectiveCompleted(205)
    SetObjectiveDisplayed(206)
EndFunction

Function Fragment_Stage_2200_Item_00()
    Actor shin = Alias_Actor_Shin_CavernsInt.GetActorReference()
    ObjectReference mine = Alias_Static_Mine.GetReference()
    ObjectReference spawnCenter = Alias_SpawnCenter_Int.GetReference()
    Quests:BS02_MQ01_Penance:QuestScript questScript = (Self as Quest) as Quests:BS02_MQ01_Penance:QuestScript
    If questScript != None
        questScript.ResolveMineBlast()
    EndIf
    If mine != None
        If ExplosionFragMine != None
            mine.PlaceAtMe(ExplosionFragMine)
        EndIf
        mine.Disable()
    EndIf
    If shin != None
        If CaptiveFaction != None
            shin.AddToFaction(CaptiveFaction)
        EndIf
        If AnimFaceArchetypeInPain != None
            shin.ChangeAnimFaceArchetype(AnimFaceArchetypeInPain)
        EndIf
    EndIf
    If spawnCenter != None
        spawnCenter.Enable()
        If questScript != None
            questScript.StartLocalWave(spawnCenter, 2300)
        EndIf
    EndIf
    SetObjectiveCompleted(206)
    SetObjectiveDisplayed(210)
EndFunction

Function Fragment_Stage_2300_Item_00()
    If BS02_MQ01_Penance_HewsenAfterBattleRemark != None
        BS02_MQ01_Penance_HewsenAfterBattleRemark.Start()
    EndIf
    SetObjectiveCompleted(210)
    SetObjectiveDisplayed(220)
EndFunction

Function Fragment_Stage_2350_Item_00()
    Actor hewsen = Alias_Actor_Hewsen_CavernsInt.GetActorReference()
    If hewsen != None && MedicBackpack != None
        If hewsen.GetItemCount(MedicBackpack) == 0
            hewsen.AddItem(MedicBackpack, 1, True)
        EndIf
        hewsen.EquipItem(MedicBackpack)
    EndIf
EndFunction

Function Fragment_Stage_2360_Item_00()
    Actor shin = Alias_Actor_Shin_CavernsInt.GetActorReference()
    Actor hewsen = Alias_Actor_Hewsen_CavernsInt.GetActorReference()
    If shin != None
        shin.EvaluatePackage()
    EndIf
    If hewsen != None
        hewsen.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_2400_Item_00()
    Actor shin = Alias_Actor_Shin_CavernsInt.GetActorReference()
    Actor hewsen = Alias_Actor_Hewsen_CavernsInt.GetActorReference()
    If shin != None
        If CaptiveFaction != None
            shin.RemoveFromFaction(CaptiveFaction)
        EndIf
        If BS02_MQ01_Penance_NoIdleDialogueKeyword != None
            shin.RemoveKeyword(BS02_MQ01_Penance_NoIdleDialogueKeyword)
        EndIf
    EndIf
    If hewsen != None && BS02_MQ01_Penance_NoIdleDialogueKeyword != None
        hewsen.RemoveKeyword(BS02_MQ01_Penance_NoIdleDialogueKeyword)
    EndIf
    SetObjectiveCompleted(220)
    SetObjectiveDisplayed(230)
EndFunction

Function Fragment_Stage_2450_Item_00()
    ObjectReference pipBoyRef = Alias_QO_Misc_PipBoy.GetReference()
    ObjectReference pipBoyMarker = Alias_Marker_PipBoy.GetReference()
    If pipBoyRef == None && pipBoyMarker != None && BS02_MQ01_Penance_PipBoy != None
        pipBoyRef = pipBoyMarker.PlaceAtMe(BS02_MQ01_Penance_PipBoy)
        If pipBoyRef != None
            Alias_QO_Misc_PipBoy.ForceRefTo(pipBoyRef)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_2500_Item_00()
    SetObjectiveCompleted(230)
    SetObjectiveDisplayed(240)
EndFunction

Function Fragment_Stage_2600_Item_00()
    SetObjectiveCompleted(240)
    SetObjectiveDisplayed(250)
EndFunction

Function Fragment_Stage_9000_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    ObjectReference pipBoyRef = Alias_QO_Misc_PipBoy.GetReference()
    Actor player = playerRef as Actor
    If playerRef != None && pipBoyRef != None && pipBoyRef.GetContainer() == playerRef
        playerRef.RemoveItem(pipBoyRef, 1, True)
    EndIf
    If player != None
        player.SetValue(BS01_ShinAwayValue, 0.0)
        player.SetValue(BS02_HewsenAwayValue, 0.0)
        player.SetValue(BS02_MarciaAwayValue, 0.0)
        player.SetValue(BS02_ArtKnappAwayValue, 0.0)
    EndIf
    If Alias_Actors_ToDisable != None
        Alias_Actors_ToDisable.DisableAll()
    EndIf
    SetObjectiveCompleted(250)
    Quests:BS02_MQ01_Penance:QuestScript questScript = (Self as Quest) as Quests:BS02_MQ01_Penance:QuestScript
    If questScript != None
        questScript.AttemptMissingHandoff(BS02_MQ02_Missing, BS02_MQ02_Missing_StartKeyword)
    EndIf
EndFunction
