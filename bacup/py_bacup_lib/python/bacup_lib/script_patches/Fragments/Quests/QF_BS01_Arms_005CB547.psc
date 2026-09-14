Function Fragment_Stage_0010_Item_00()
    Actor player = Game.GetPlayer()
    If player && BS01_MQ04_Arms_DaggerEnableValue
        player.SetValue(BS01_MQ04_Arms_DaggerEnableValue, 1.0)
    EndIf
    If Alias_Dagger_EnableMarker && Alias_Dagger_EnableMarker.GetReference()
        Alias_Dagger_EnableMarker.GetReference().Enable()
    EndIf
    If Alias_Dagger && Alias_Dagger.GetReference()
        Alias_Dagger.GetReference().Enable()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    Actor player = Game.GetPlayer()
    If Alias_Player && player
        Alias_Player.ForceRefIfEmpty(player)
    EndIf
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(25)
    If Alias_Villagers
        int villagerCount = Alias_Villagers.GetCount()
        If Alias_Villager01 && villagerCount > 0
            Alias_Villager01.ForceRefIfEmpty(Alias_Villagers.GetAt(0))
        EndIf
        If Alias_Villager02 && villagerCount > 1
            Alias_Villager02.ForceRefIfEmpty(Alias_Villagers.GetAt(1))
        EndIf
        If Alias_Villager03 && villagerCount > 2
            Alias_Villager03.ForceRefIfEmpty(Alias_Villagers.GetAt(2))
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0310_Item_00()
    If Alias_Villager01 && Alias_Villager01.GetReference() && BS01_MQ04_Arms_PlayerTalkedToVillager
        Alias_Villager01.GetReference().AddKeyword(BS01_MQ04_Arms_PlayerTalkedToVillager)
    EndIf
    SetObjectiveDisplayed(25, True, True)
    If IsStageDone(320) && IsStageDone(330) && !IsStageDone(350)
        SetStage(350)
    EndIf
EndFunction

Function Fragment_Stage_0320_Item_00()
    If Alias_Villager02 && Alias_Villager02.GetReference() && BS01_MQ04_Arms_PlayerTalkedToVillager
        Alias_Villager02.GetReference().AddKeyword(BS01_MQ04_Arms_PlayerTalkedToVillager)
    EndIf
    SetObjectiveDisplayed(25, True, True)
    If IsStageDone(310) && IsStageDone(330) && !IsStageDone(350)
        SetStage(350)
    EndIf
EndFunction

Function Fragment_Stage_0330_Item_00()
    If Alias_Villager03 && Alias_Villager03.GetReference() && BS01_MQ04_Arms_PlayerTalkedToVillager
        Alias_Villager03.GetReference().AddKeyword(BS01_MQ04_Arms_PlayerTalkedToVillager)
    EndIf
    SetObjectiveDisplayed(25, True, True)
    If IsStageDone(310) && IsStageDone(320) && !IsStageDone(350)
        SetStage(350)
    EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
    SetObjectiveCompleted(25)
    SetObjectiveDisplayed(28)
EndFunction

Function Fragment_Stage_0375_Item_00()
    Actor player = Game.GetPlayer()
    SetObjectiveCompleted(28)
    SetObjectiveDisplayed(30)
    If player && BS01_MQ04_Arms_DaggerEnableValue
        player.SetValue(BS01_MQ04_Arms_DaggerEnableValue, 1.0)
    EndIf
    If Alias_Dagger_EnableMarker && Alias_Dagger_EnableMarker.GetReference()
        Alias_Dagger_EnableMarker.GetReference().Enable()
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0450_Item_00()
    SetObjectiveCompleted(42)
    SetObjectiveDisplayed(45)
    SetObjectiveDisplayed(46)
EndFunction

Function Fragment_Stage_0500_Item_00()
    If !IsStageDone(515)
        SetStage(515)
    EndIf
EndFunction

Function Fragment_Stage_0515_Item_00()
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(42)
EndFunction

Function Fragment_Stage_0520_Item_00()
    If Alias_BloodEagles
        Alias_BloodEagles.DisableAll(False)
    EndIf
EndFunction

Function Fragment_Stage_0525_Item_00()
    ObjectReference keyGuardRef = None
    If Alias_KeyGuard
        keyGuardRef = Alias_KeyGuard.GetReference()
    EndIf
    If keyGuardRef
        If Alias_KeyDesk && Alias_KeyDesk.GetReference()
            keyGuardRef.MoveTo(Alias_KeyDesk.GetReference())
        EndIf
        If BS01_Arms_ThroneRoomKey && keyGuardRef.GetItemCount(BS01_Arms_ThroneRoomKey) < 1
            keyGuardRef.AddItem(BS01_Arms_ThroneRoomKey, 1, True)
        EndIf
        keyGuardRef.Enable()
    EndIf
    If BS01_MQ04_Arms_KeyGuardScene
        BS01_MQ04_Arms_KeyGuardScene.Start()
    EndIf
    SetObjectiveCompleted(42)
    SetObjectiveDisplayed(45)
    SetObjectiveDisplayed(46)
EndFunction

Function Fragment_Stage_0550_Item_00()
    ObjectReference keyGuardRef = None
    If Alias_KeyGuard
        keyGuardRef = Alias_KeyGuard.GetReference()
    EndIf
    If keyGuardRef && BS01_Arms_ThroneRoomKey && keyGuardRef.GetItemCount(BS01_Arms_ThroneRoomKey) < 1
        keyGuardRef.AddItem(BS01_Arms_ThroneRoomKey, 1, True)
    EndIf
    SetObjectiveCompleted(45)
EndFunction

Function Fragment_Stage_0575_Item_00()
    SetObjectiveCompleted(45)
    SetObjectiveCompleted(46)
    SetObjectiveDisplayed(48)
EndFunction

Function Fragment_Stage_0600_Item_00()
    Actor player = Game.GetPlayer()
    ObjectReference daggerRef = None
    If Alias_Dagger
        daggerRef = Alias_Dagger.GetReference()
    EndIf
    If player && BS01_MQ04_Arms_DaggerEnableValue
        player.SetValue(BS01_MQ04_Arms_DaggerEnableValue, 1.0)
    EndIf
    If daggerRef && BS01_MQ04_Arms_DaggerInThroneKeyword
        daggerRef.AddKeyword(BS01_MQ04_Arms_DaggerInThroneKeyword)
    EndIf
    SetObjectiveCompleted(48)
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0610_Item_00()
    If Alias_SupplyCrate && Alias_SupplyCrate.GetReference()
        Alias_SupplyCrate.GetReference().Enable()
    EndIf
    If Alias_WeaponsCache && Alias_WeaponsCache.GetReference()
        Alias_WeaponsCache.GetReference().Enable()
    EndIf
EndFunction

Function Fragment_Stage_0625_Item_00()
    If BS01_Arms_DaggerScene
        BS01_Arms_DaggerScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0640_Item_00()
    Actor player = Game.GetPlayer()
    If player && Caps001 && player.GetItemCount(Caps001) >= 300
        player.RemoveItem(Caps001, 300, True)
    EndIf
EndFunction

Function Fragment_Stage_0650_Item_00()
    Actor player = Game.GetPlayer()
    If player && BS01_MQ04_Arms_DaggerResolution
        player.SetValue(BS01_MQ04_Arms_DaggerResolution, 3.0)
    EndIf
    If !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    Actor player = Game.GetPlayer()
    Actor daggerActor = None
    If Alias_Dagger
        daggerActor = Alias_Dagger.GetReference() as Actor
    EndIf
    If player && BS01_MQ04_Arms_DaggerResolution && player.GetValue(BS01_MQ04_Arms_DaggerResolution) != 3.0
        player.SetValue(BS01_MQ04_Arms_DaggerResolution, 2.0)
    EndIf
    If daggerActor && PlayerEnemyFaction
        daggerActor.RemoveFromFaction(PlayerEnemyFaction)
    EndIf
    If Alias_Lieutenants && PlayerEnemyFaction
        Alias_Lieutenants.RemoveFromFaction(PlayerEnemyFaction)
    EndIf
    If player && BS01_Arms_SupplyCrateKey && player.GetItemCount(BS01_Arms_SupplyCrateKey) < 1
        player.AddItem(BS01_Arms_SupplyCrateKey, 1, True)
    EndIf
    SetObjectiveCompleted(50)
    If !IsStageDone(860)
        SetStage(860)
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    Actor player = Game.GetPlayer()
    Actor daggerActor = None
    If Alias_Dagger
        daggerActor = Alias_Dagger.GetReference() as Actor
    EndIf
    If player && BS01_MQ04_Arms_DaggerResolution
        player.SetValue(BS01_MQ04_Arms_DaggerResolution, 1.0)
    EndIf
    If BS01_MQ04_Arms_LieutenantFaction && PlayerFaction
        BS01_MQ04_Arms_LieutenantFaction.SetEnemy(PlayerFaction)
    EndIf
    If BloodEagleFaction && BS01_MQ04_Arms_LieutenantFaction
        BloodEagleFaction.SetAlly(BS01_MQ04_Arms_LieutenantFaction)
    EndIf
    If daggerActor && PlayerEnemyFaction
        daggerActor.AddToFaction(PlayerEnemyFaction)
    EndIf
    If Alias_Lieutenants && PlayerEnemyFaction
        Alias_Lieutenants.AddToFaction(PlayerEnemyFaction)
    EndIf
    If daggerActor && player
        daggerActor.StartCombat(player, True)
    EndIf
    If Alias_Lieutenants && player
        Alias_Lieutenants.StartCombatAll(player)
    EndIf
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(65)
EndFunction

Function Fragment_Stage_0850_Item_00()
    ObjectReference daggerRef = None
    If Alias_Dagger
        daggerRef = Alias_Dagger.GetReference()
    EndIf
    If daggerRef && BS01_Arms_SupplyCrateKey && daggerRef.GetItemCount(BS01_Arms_SupplyCrateKey) < 1
        daggerRef.AddItem(BS01_Arms_SupplyCrateKey, 1, True)
    EndIf
    SetObjectiveCompleted(65)
    SetObjectiveDisplayed(68)
EndFunction

Function Fragment_Stage_0860_Item_00()
    SetObjectiveCompleted(68)
    If !IsStageDone(870)
        SetStage(870)
    EndIf
EndFunction

Function Fragment_Stage_0870_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(54)
    SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0875_Item_00()
    Actor player = Game.GetPlayer()
    If player && BS01_MQ04_Arms_PlayerFoundWeaponsCache
        player.AddKeyword(BS01_MQ04_Arms_PlayerFoundWeaponsCache)
    EndIf
    SetObjectiveCompleted(54)
    If IsStageDone(900) && !IsStageDone(910)
        SetStage(910)
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(60)
    If IsStageDone(875) && !IsStageDone(910)
        SetStage(910)
    EndIf
EndFunction

Function Fragment_Stage_0910_Item_00()
    SetObjectiveCompleted(54)
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(70)
    SetObjectiveDisplayed(75)
EndFunction

Function Fragment_Stage_0920_Item_00()
    Actor player = Game.GetPlayer()
    If player && BS01_MQ04_Arms_WeaponChoice
        player.SetValue(BS01_MQ04_Arms_WeaponChoice, 1.0)
    EndIf
    If player
        ObjectReference suppliesRef = None
        If Alias_Supplies
            suppliesRef = Alias_Supplies.GetReference()
        EndIf
        If suppliesRef && suppliesRef.GetContainer() == player
            player.RemoveItem(suppliesRef, 1, True)
        ElseIf BS01_MQ04_Arms_Supplies
            player.RemoveItem(BS01_MQ04_Arms_Supplies, 1, True)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0940_Item_00()
    Actor player = Game.GetPlayer()
    If player && BS01_MQ04_Arms_WeaponChoice
        player.SetValue(BS01_MQ04_Arms_WeaponChoice, 2.0)
    EndIf
    If player
        ObjectReference suppliesRef = None
        ObjectReference weaponsRef = None
        If Alias_Supplies
            suppliesRef = Alias_Supplies.GetReference()
        EndIf
        If Alias_QO_WeaponsCache
            weaponsRef = Alias_QO_WeaponsCache.GetReference()
        EndIf
        If suppliesRef && suppliesRef.GetContainer() == player
            player.RemoveItem(suppliesRef, 1, True)
        ElseIf BS01_MQ04_Arms_Supplies
            player.RemoveItem(BS01_MQ04_Arms_Supplies, 1, True)
        EndIf
        If weaponsRef && weaponsRef.GetContainer() == player
            player.RemoveItem(weaponsRef, 1, True)
        ElseIf BS01_MQ04_Arms_WeaponCache
            player.RemoveItem(BS01_MQ04_Arms_WeaponCache, 1, True)
        EndIf
        If BS01_MQ04_Arms_GaveWeaponsKeyword
            player.AddKeyword(BS01_MQ04_Arms_GaveWeaponsKeyword)
        EndIf
    EndIf
    If Alias_Jennie_FortAtlas && Alias_Jennie_FortAtlas.GetReference() && BS01_MQ04_Arms_GaveWeaponsKeyword
        Alias_Jennie_FortAtlas.GetReference().AddKeyword(BS01_MQ04_Arms_GaveWeaponsKeyword)
    EndIf
EndFunction

Function Fragment_Stage_0945_Item_00()
    Actor player = Game.GetPlayer()
    If player && BS01_MQ04_Arms_PlayerTurnedInJennie
        player.AddKeyword(BS01_MQ04_Arms_PlayerTurnedInJennie)
    EndIf
    SetObjectiveCompleted(70)
    SetObjectiveCompleted(75)
    SetObjectiveDisplayed(80)
EndFunction

Function Fragment_Stage_0950_Item_00()
    Actor rahmaniRef = None
    Actor shinRef = None
    If Alias_Rahmani
        rahmaniRef = Alias_Rahmani.GetActorReference()
    EndIf
    If Alias_Shin
        shinRef = Alias_Shin.GetActorReference()
    EndIf
    If rahmaniRef && Alias_XMarker_Rahmani && Alias_XMarker_Rahmani.GetReference()
        rahmaniRef.MoveTo(Alias_XMarker_Rahmani.GetReference())
    EndIf
    If shinRef && Alias_XMarker_Shin && Alias_XMarker_Shin.GetReference()
        shinRef.MoveTo(Alias_XMarker_Shin.GetReference())
    EndIf
    If rahmaniRef
        rahmaniRef.EvaluatePackage()
    EndIf
    If shinRef
        shinRef.EvaluatePackage()
    EndIf
    If BS01_MQ04_Arms_FinalSceneConversation
        BS01_MQ04_Arms_FinalSceneConversation.Start()
    EndIf
EndFunction

Function Fragment_Stage_0955_Item_00()
    If BS01_MQ04_Arms_FinalSceneConversation
        BS01_MQ04_Arms_FinalSceneConversation.Stop()
    EndIf
    If BS01_MQ04_Arms_FinalScene
        BS01_MQ04_Arms_FinalScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0960_Item_00()
    SetObjectiveCompleted(80)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor player = Game.GetPlayer()
    ObjectReference daggerRef = None
    Bool accepted = False
    If Alias_Player && player
        Alias_Player.ForceRefIfEmpty(player)
    EndIf
    If Alias_Dagger
        daggerRef = Alias_Dagger.GetReference()
    EndIf
    If daggerRef && BS01_MQ04_Arms_DaggerInThroneKeyword
        daggerRef.RemoveKeyword(BS01_MQ04_Arms_DaggerInThroneKeyword)
    EndIf
    If player && BS01_MQ04_Arms_DaggerEnableValue
        player.SetValue(BS01_MQ04_Arms_DaggerEnableValue, 0.0)
    EndIf
    If Alias_Dagger_EnableMarker && Alias_Dagger_EnableMarker.GetReference()
        Alias_Dagger_EnableMarker.GetReference().Disable()
    EndIf
    If BS01_MQ05_Raiders
        accepted = BS01_MQ05_Raiders.IsRunning() || BS01_MQ05_Raiders.IsCompleted()
    EndIf
    If !accepted && player && BS01_MQ05_Raiders_StartKeyword
        accepted = BS01_MQ05_Raiders_StartKeyword.SendStoryEventAndWait(None, player, player)
    EndIf
    If !accepted && BS01_MQ05_Raiders
        accepted = BS01_MQ05_Raiders.IsRunning() || BS01_MQ05_Raiders.IsCompleted()
    EndIf
    If accepted
        Stop()
    Else
        StartTimer(5.0, 9000)
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != 9000 || !IsStageDone(9000)
        Return
    EndIf

    Actor player = Game.GetPlayer()
    Bool accepted = False
    If Alias_Player && player
        Alias_Player.ForceRefIfEmpty(player)
    EndIf
    If BS01_MQ05_Raiders
        accepted = BS01_MQ05_Raiders.IsRunning() || BS01_MQ05_Raiders.IsCompleted()
    EndIf
    If !accepted && player && BS01_MQ05_Raiders_StartKeyword
        accepted = BS01_MQ05_Raiders_StartKeyword.SendStoryEventAndWait(None, player, player)
    EndIf
    If !accepted && BS01_MQ05_Raiders
        accepted = BS01_MQ05_Raiders.IsRunning() || BS01_MQ05_Raiders.IsCompleted()
    EndIf
    If accepted
        Stop()
    Else
        StartTimer(5.0, 9000)
    EndIf
EndEvent
