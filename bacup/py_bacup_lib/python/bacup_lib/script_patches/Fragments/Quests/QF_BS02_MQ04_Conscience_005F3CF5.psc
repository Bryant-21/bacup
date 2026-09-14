Function Fragment_Stage_0010_Item_00()
    Actor atriumBloodEagle = Alias_Enemy_AtriumBloodEagle.GetActorReference()
    If atriumBloodEagle
        atriumBloodEagle.RemoveFromFaction(PlayerAlly)
        atriumBloodEagle.AddToFaction(PlayerEnemy)
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef
        playerRef.SetValue(ValdezDungeon_AV, 1.0)
        playerRef.SetValue(WoodsPresent_AV, 1.0)
    EndIf
    ObjectReference dungeonEncounters = Alias_EM_ValdezDungeon.GetReference()
    If dungeonEncounters
        dungeonEncounters.Enable()
    EndIf
    ObjectReference vaultDoors = Alias_EM_VaultDoors.GetReference()
    If vaultDoors
        vaultDoors.Enable()
    EndIf
    ObjectReference hellcatCorpses = Alias_EM_HellcatCorpses.GetReference()
    If hellcatCorpses
        hellcatCorpses.Enable()
    EndIf
    ObjectReference blackburnEncounter = Alias_EM_Blackburn.GetReference()
    If blackburnEncounter
        blackburnEncounter.Enable()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef
        Alias_Player.ForceRefIfEmpty(playerRef)
        playerRef.SetValue(WoodsPresent_AV, 1.0)
    EndIf
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0150_Item_00()
    Actor rahmani = Alias_Actor_Rahmani.GetActorReference()
    ObjectReference rahmaniMarker = Alias_Marker_Rahmani_Intro.GetReference()
    If rahmani && rahmaniMarker
        rahmani.MoveTo(rahmaniMarker)
    EndIf
    Actor valdez = Alias_Actor_ValdezFollower.GetActorReference()
    ObjectReference valdezMarker = Alias_Marker_Valdez_Office.GetReference()
    If valdez && valdezMarker
        valdez.MoveTo(valdezMarker)
    EndIf
EndFunction

Function Fragment_Stage_0160_Item_00()
    If Intro_Ambient_Scene && !Intro_Ambient_Scene.IsPlaying()
        Intro_Ambient_Scene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(200)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(ValdezAwayValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(200)
    SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(300)
    SetObjectiveDisplayed(400)
    ObjectReference hellcatCorpses = Alias_EM_HellcatCorpses.GetReference()
    If hellcatCorpses
        hellcatCorpses.Enable()
    EndIf
    ObjectReference dungeonEncounters = Alias_EM_ValdezDungeon.GetReference()
    If dungeonEncounters
        dungeonEncounters.Enable()
    EndIf
EndFunction

Function Fragment_Stage_0450_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(WoodsPresent_AV, 0.0)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(400)
    SetObjectiveDisplayed(500)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(WoodsPresent_AV, 0.0)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(500)
    SetObjectiveDisplayed(600)
    Actor valdez = Alias_Actor_ValdezFollower.GetActorReference()
    ObjectReference arrivalMarker = Alias_Marker_Valdez_TeleportOffice.GetReference()
    If valdez
        valdez.Enable()
        If arrivalMarker
            valdez.MoveTo(arrivalMarker)
        EndIf
        valdez.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(600)
    SetObjectiveDisplayed(700)
EndFunction

Function Fragment_Stage_0710_Item_00()
    SetObjectiveCompleted(700)
    SetObjectiveDisplayed(710)
    If Valdez_EntranceDoorScene && !Valdez_EntranceDoorScene.IsPlaying()
        Valdez_EntranceDoorScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0750_Item_00()
    SetObjectiveCompleted(710)
    SetObjectiveDisplayed(750)
    SetObjectiveDisplayed(815)
    ObjectReference entranceDoor = Alias_Door_Entrance.GetReference()
    If entranceDoor
        entranceDoor.Unlock()
        entranceDoor.SetOpen(True)
    EndIf
    ObjectReference dummyTerminal = Alias_Dummy_EntranceTerminal.GetReference()
    If dummyTerminal
        dummyTerminal.Disable()
    EndIf
    ObjectReference entranceTerminal = Alias_Terminal_EntranceTerminal02.GetReference()
    If entranceTerminal
        entranceTerminal.Enable()
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(815)
    SetObjectiveDisplayed(800)
    ObjectReference bloodEagleEncounter = Alias_EM_Atrium_BloodEagles.GetReference()
    If bloodEagleEncounter
        bloodEagleEncounter.Enable()
    EndIf
    ObjectReference localEncounter = Alias_EM_Atrium_NonQuestCombat.GetReference()
    If localEncounter
        localEncounter.Enable()
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(800)
    SetObjectiveDisplayed(810)
    ObjectReference bloodEagleEncounter = Alias_EM_Atrium_BloodEagles.GetReference()
    If bloodEagleEncounter
        bloodEagleEncounter.Disable()
    EndIf
    If BlackburnAmbient_Atrium && !BlackburnAmbient_Atrium.IsPlaying()
        BlackburnAmbient_Atrium.Start()
    EndIf
EndFunction

Function Fragment_Stage_0930_Item_00()
    SetObjectiveDisplayed(810)
    If BlackburnAmbient_Atrium && !BlackburnAmbient_Atrium.IsPlaying()
        BlackburnAmbient_Atrium.Start()
    EndIf
EndFunction

Function Fragment_Stage_0950_Item_00()
    SetObjectiveCompleted(810)
    SetObjectiveDisplayed(820)
    If ValdezAmbient_Atrium && !ValdezAmbient_Atrium.IsPlaying()
        ValdezAmbient_Atrium.Start()
    EndIf
EndFunction

Function Fragment_Stage_0954_Item_00()
    SetObjectiveDisplayed(830)
    If ValdezAmbient_Atrium && !ValdezAmbient_Atrium.IsPlaying()
        ValdezAmbient_Atrium.Start()
    EndIf
EndFunction

Function Fragment_Stage_0955_Item_00()
    SetObjectiveCompleted(820)
    SetObjectiveCompleted(830)
    SetObjectiveDisplayed(900)
    ObjectReference mainframeEncounters = Alias_EM_MainframeEnemies.GetReference()
    If mainframeEncounters
        mainframeEncounters.Enable()
    EndIf
EndFunction

Function Fragment_Stage_0960_Item_00()
    If BlackburnAmbient_Mainframe && !BlackburnAmbient_Mainframe.IsPlaying()
        BlackburnAmbient_Mainframe.Start()
    EndIf
EndFunction

Function Fragment_Stage_0965_Item_00()
    If ValdezAmbient_Mainframe && !ValdezAmbient_Mainframe.IsPlaying()
        ValdezAmbient_Mainframe.Start()
    EndIf
EndFunction

Function Fragment_Stage_0981_Item_00()
    If IsStageDone(982) && IsStageDone(983) && !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_0982_Item_00()
    If IsStageDone(981) && IsStageDone(983) && !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_0983_Item_00()
    If IsStageDone(981) && IsStageDone(982) && !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(900)
    SetObjectiveDisplayed(1000)
    SetObjectiveDisplayed(1050)
    ObjectReference mainframeEncounters = Alias_EM_MainframeEnemies.GetReference()
    If mainframeEncounters
        mainframeEncounters.Disable()
    EndIf
EndFunction

Function Fragment_Stage_1050_Item_00()
    SetObjectiveCompleted(1050)
    ObjectReference reactorMarker = Alias_Marker_ReactorTerminal.GetReference()
    If reactorMarker && ElectricalExplosion
        reactorMarker.PlaceAtMe(ElectricalExplosion)
    EndIf
    ObjectReference reactorTerminal = Alias_Terminal_ReactorControlDoor.GetReference()
    If reactorTerminal
        reactorTerminal.Disable()
    EndIf
    ObjectReference reactorDoor = Alias_Door_ReactorControl.GetReference()
    If reactorDoor
        reactorDoor.Unlock()
        reactorDoor.SetOpen(True)
    EndIf
    If !IsStageDone(1100)
        SetStage(1100)
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveCompleted(1000)
    SetObjectiveCompleted(1050)
    SetObjectiveDisplayed(1100)
    ObjectReference reactorDoor = Alias_Door_ReactorControl.GetReference()
    If reactorDoor
        reactorDoor.Unlock()
        reactorDoor.SetOpen(True)
    EndIf
    ObjectReference reactorEncounters = Alias_EM_ReactorEnemies.GetReference()
    If reactorEncounters
        reactorEncounters.Enable()
    EndIf
EndFunction

Function Fragment_Stage_1110_Item_00()
    If BlackburnAmbient_Reactor && !BlackburnAmbient_Reactor.IsPlaying()
        BlackburnAmbient_Reactor.Start()
    EndIf
EndFunction

Function Fragment_Stage_1150_Item_00()
    If IsStageDone(1151) && !IsStageDone(1200)
        SetStage(1200)
    EndIf
EndFunction

Function Fragment_Stage_1151_Item_00()
    If IsStageDone(1150) && !IsStageDone(1200)
        SetStage(1200)
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveCompleted(1100)
    SetObjectiveDisplayed(1200)
    ObjectReference reactorEncounters = Alias_EM_ReactorEnemies.GetReference()
    If reactorEncounters
        reactorEncounters.Disable()
    EndIf
    If ValdezAmbient_Reactor && !ValdezAmbient_Reactor.IsPlaying()
        ValdezAmbient_Reactor.Start()
    EndIf
EndFunction

Function Fragment_Stage_1240_Item_00()
    SetObjectiveDisplayed(1250)
EndFunction

Function Fragment_Stage_1250_Item_00()
    If Tally_IntercomScene && !Tally_IntercomScene.IsPlaying()
        Tally_IntercomScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    SetObjectiveCompleted(1250)
    SetObjectiveDisplayed(1400)
    ObjectReference cryoDoor = Alias_Door_CryoBay.GetReference()
    If cryoDoor
        cryoDoor.Unlock()
        cryoDoor.SetOpen(True)
    EndIf
EndFunction

Function Fragment_Stage_1400_Item_00()
    ObjectReference tallyEncounter = Alias_EM_TallyLang.GetReference()
    If tallyEncounter
        tallyEncounter.Enable()
    EndIf
    ObjectReference cryoEncounters = Alias_EM_CryoEnemies.GetReference()
    If cryoEncounters
        cryoEncounters.Enable()
    EndIf
    Actor playerRef = Game.GetPlayer()
    ReferenceAlias cryobotSpawnAlias = GetAlias(97) as ReferenceAlias
    ObjectReference cryobotSpawn = None
    If cryobotSpawnAlias
        cryobotSpawn = cryobotSpawnAlias.GetReference()
    EndIf
    ActorBase cryobotBase = Game.GetFormFromFile(0x00450123, "SeventySix.esm") as ActorBase
    If playerRef && cryobotSpawn && cryobotBase && Alias_Enemies_Cryobots && Alias_Enemies_Cryobots.GetCount() == 0
        Actor cryobotOne = cryobotSpawn.PlaceActorAtMe(cryobotBase, 2)
        Actor cryobotTwo = cryobotSpawn.PlaceActorAtMe(cryobotBase, 2)
        If cryobotOne && cryobotTwo
            Alias_Enemies_Cryobots.AddRef(cryobotOne)
            Alias_Enemies_Cryobots.AddRef(cryobotTwo)
            RegisterForRemoteEvent(cryobotOne, "OnDeath")
            RegisterForRemoteEvent(cryobotTwo, "OnDeath")
            cryobotOne.StartCombat(playerRef)
            cryobotTwo.StartCombat(playerRef)
            cryobotOne.EvaluatePackage()
            cryobotTwo.EvaluatePackage()
        Else
            If cryobotOne
                cryobotOne.Disable()
                cryobotOne.Delete()
            EndIf
            If cryobotTwo
                cryobotTwo.Disable()
                cryobotTwo.Delete()
            EndIf
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
    SetObjectiveCompleted(1400)
    SetObjectiveDisplayed(1300)
    If TallyPreFightScene && !TallyPreFightScene.IsPlaying()
        TallyPreFightScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1550_Item_00()
    SetObjectiveCompleted(1300)
    SetObjectiveDisplayed(1550)
    Actor tally = Alias_Actor_TallyLang.GetActorReference()
    If tally
        tally.RemoveFromFaction(PlayerAlly)
        tally.AddToFaction(PlayerEnemy)
        tally.EvaluatePackage()
    EndIf
    Int i = 0
    While i < Alias_Actors_TallysBloodEagles.GetCount()
        Actor bloodEagle = Alias_Actors_TallysBloodEagles.GetAt(i) as Actor
        If bloodEagle
            bloodEagle.RemoveFromFaction(PlayerAlly)
            bloodEagle.AddToFaction(PlayerEnemy)
            bloodEagle.EvaluatePackage()
        EndIf
        i += 1
    EndWhile
EndFunction

Function Fragment_Stage_1560_Item_00()
    SetObjectiveCompleted(1550)
    SetObjectiveDisplayed(1560)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef && playerRef.GetItemCount(SecurityKeycard) == 0
        playerRef.AddItem(SecurityKeycard, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_1600_Item_00()
    SetObjectiveCompleted(1300)
    SetObjectiveDisplayed(1600)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        If playerRef.GetItemCount(SecurityKeycard) == 0
            playerRef.AddItem(SecurityKeycard, 1, False)
        EndIf
        playerRef.SetValue(TallyGaveKeycard, 1.0)
    EndIf
    Actor tally = Alias_Actor_TallyLang.GetActorReference()
    If tally
        tally.RemoveFromFaction(PlayerEnemy)
        tally.AddToFaction(PlayerAlly)
        tally.EvaluatePackage()
    EndIf
    Int i = 0
    While i < Alias_Actors_TallysBloodEagles.GetCount()
        Actor bloodEagle = Alias_Actors_TallysBloodEagles.GetAt(i) as Actor
        If bloodEagle
            bloodEagle.RemoveFromFaction(PlayerEnemy)
            bloodEagle.AddToFaction(PlayerAlly)
            bloodEagle.EvaluatePackage()
        EndIf
        i += 1
    EndWhile
    ObjectReference cryoEncounters = Alias_EM_CryoEnemies.GetReference()
    If cryoEncounters
        cryoEncounters.Disable()
    EndIf
    If ValdezPostTallyScene && !ValdezPostTallyScene.IsPlaying()
        ValdezPostTallyScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1600_Item_01()
    SetObjectiveCompleted(1550)
    SetObjectiveCompleted(1560)
    SetObjectiveDisplayed(1600)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef && playerRef.GetItemCount(SecurityKeycard) == 0
        playerRef.AddItem(SecurityKeycard, 1, False)
    EndIf
    ObjectReference cryoEncounters = Alias_EM_CryoEnemies.GetReference()
    If cryoEncounters
        cryoEncounters.Disable()
    EndIf
    ObjectReference tallyEncounter = Alias_EM_TallyLang.GetReference()
    If tallyEncounter
        tallyEncounter.Disable()
    EndIf
    If Valdez_PostFight_Scene && !Valdez_PostFight_Scene.IsPlaying()
        Valdez_PostFight_Scene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1605_Item_00()
    SetObjectiveDisplayed(1600)
EndFunction

Function Fragment_Stage_1620_Item_00()
    Actor tally = Alias_Actor_TallyLang.GetActorReference()
    If tally
        tally.Disable()
    EndIf
    Int i = 0
    While i < Alias_Actors_TallysBloodEagles.GetCount()
        ObjectReference bloodEagle = Alias_Actors_TallysBloodEagles.GetAt(i)
        If bloodEagle
            bloodEagle.Disable()
        EndIf
        i += 1
    EndWhile
    ObjectReference tallyEncounter = Alias_EM_TallyLang.GetReference()
    If tallyEncounter
        tallyEncounter.Disable()
    EndIf
EndFunction

Function Fragment_Stage_1650_Item_00()
    If IsStageDone(1660) && IsStageDone(1670) && !IsStageDone(1700)
        SetStage(1700)
    EndIf
EndFunction

Function Fragment_Stage_1660_Item_00()
    If IsStageDone(1650) && IsStageDone(1670) && !IsStageDone(1700)
        SetStage(1700)
    EndIf
EndFunction

Function Fragment_Stage_1670_Item_00()
    If IsStageDone(1650) && IsStageDone(1660) && !IsStageDone(1700)
        SetStage(1700)
    EndIf
EndFunction

Function Fragment_Stage_1700_Item_00()
    SetObjectiveCompleted(1600)
    SetObjectiveDisplayed(1700)
    ObjectReference cryoEncounters = Alias_EM_CryoEnemies.GetReference()
    If cryoEncounters
        cryoEncounters.Disable()
    EndIf
    If BlackburnAmbient_Cryo && !BlackburnAmbient_Cryo.IsPlaying()
        BlackburnAmbient_Cryo.Start()
    EndIf
EndFunction

Function Fragment_Stage_1800_Item_00()
    SetObjectiveCompleted(1700)
    SetObjectiveDisplayed(1800)
    SetObjectiveDisplayed(1840)
    ObjectReference researchEncounters = Alias_EM_ResearchEnemies.GetReference()
    If researchEncounters
        researchEncounters.Enable()
    EndIf
    If ValdezAmbient_Research && !ValdezAmbient_Research.IsPlaying()
        ValdezAmbient_Research.Start()
    EndIf
EndFunction

Function Fragment_Stage_1850_Item_00()
    If IsStageDone(1855) && IsStageDone(1860) && !IsStageDone(1900)
        SetStage(1900)
    EndIf
EndFunction

Function Fragment_Stage_1855_Item_00()
    If IsStageDone(1850) && IsStageDone(1860) && !IsStageDone(1900)
        SetStage(1900)
    EndIf
EndFunction

Function Fragment_Stage_1860_Item_00()
    If IsStageDone(1850) && IsStageDone(1855) && !IsStageDone(1900)
        SetStage(1900)
    EndIf
EndFunction

Function Fragment_Stage_1870_Item_00()
    If BlackburnAmbient_ResearchLab && !BlackburnAmbient_ResearchLab.IsPlaying()
        BlackburnAmbient_ResearchLab.Start()
    EndIf
EndFunction

Function Fragment_Stage_1875_Item_00()
    If ValdezAmbient_ResearchLab && !ValdezAmbient_ResearchLab.IsPlaying()
        ValdezAmbient_ResearchLab.Start()
    EndIf
EndFunction

Function Fragment_Stage_1900_Item_00()
    SetObjectiveCompleted(1800)
    SetObjectiveDisplayed(1840)
EndFunction

Function Fragment_Stage_1900_Item_01()
    SetObjectiveCompleted(1800)
    SetObjectiveCompleted(1840)
    SetObjectiveCompleted(1850)
    SetObjectiveDisplayed(1860)
EndFunction

Function Fragment_Stage_1900_Item_02()
    SetObjectiveCompleted(1800)
    SetObjectiveCompleted(1840)
    SetObjectiveCompleted(1850)
    SetObjectiveCompleted(1860)
    SetObjectiveDisplayed(1900)
EndFunction

Function Fragment_Stage_1900_Item_03()
    SetObjectiveCompleted(1800)
    SetObjectiveCompleted(1840)
    SetObjectiveDisplayed(1850)
EndFunction

Function Fragment_Stage_1900_Item_04()
    SetObjectiveCompleted(1800)
    SetObjectiveCompleted(1840)
    SetObjectiveCompleted(1850)
    SetObjectiveCompleted(1860)
    SetObjectiveCompleted(1900)
    If !IsStageDone(2100)
        SetStage(2100)
    EndIf
EndFunction

Function Fragment_Stage_1940_Item_00()
    SetObjectiveDisplayed(1840)
    If CassieShoutScene && !CassieShoutScene.IsPlaying()
        CassieShoutScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1940_Item_01()
    SetObjectiveCompleted(1800)
    SetObjectiveDisplayed(1840)
    If CassieShoutScene && !CassieShoutScene.IsPlaying()
        CassieShoutScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1950_Item_00()
    SetObjectiveCompleted(1840)
    SetObjectiveDisplayed(1850)
    Actor valdez = Alias_Actor_ValdezFollower.GetActorReference()
    ObjectReference windowMarker = Alias_Marker_Valdez_CassieWindow.GetReference()
    If valdez && windowMarker
        valdez.MoveTo(windowMarker)
        valdez.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1950_Item_01()
    SetObjectiveCompleted(1800)
    SetObjectiveCompleted(1840)
    SetObjectiveDisplayed(1850)
    Actor valdez = Alias_Actor_ValdezFollower.GetActorReference()
    ObjectReference windowMarker = Alias_Marker_Valdez_CassieWindow.GetReference()
    If valdez && windowMarker
        valdez.MoveTo(windowMarker)
        valdez.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1952_Item_00()
    If CassieGreetScene && !CassieGreetScene.IsPlaying()
        CassieGreetScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1960_Item_00()
    SetObjectiveCompleted(1850)
    SetObjectiveDisplayed(1860)
    Actor valdez = Alias_Actor_ValdezFollower.GetActorReference()
    ObjectReference cassieMarker = Alias_Marker_Valdez_Cassie.GetReference()
    If valdez && cassieMarker
        valdez.MoveTo(cassieMarker)
        valdez.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1960_Item_01()
    SetObjectiveCompleted(1800)
    SetObjectiveCompleted(1850)
    SetObjectiveDisplayed(1860)
    Actor valdez = Alias_Actor_ValdezFollower.GetActorReference()
    ObjectReference cassieMarker = Alias_Marker_Valdez_Cassie.GetReference()
    If valdez && cassieMarker
        valdez.MoveTo(cassieMarker)
        valdez.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_1970_Item_00()
    ObjectReference chamberDoor = Alias_Door_TestChamber_01.GetReference()
    If chamberDoor
        chamberDoor.Unlock()
        chamberDoor.SetOpen(True)
    EndIf
    ObjectReference utilityDoor = Alias_Door_TestChamber_01_Utility.GetReference()
    If utilityDoor
        utilityDoor.Unlock()
        utilityDoor.SetOpen(True)
    EndIf
EndFunction

Function Fragment_Stage_1980_Item_00()
    ObjectReference chamberDoor = Alias_Door_TestChamber_04.GetReference()
    If chamberDoor
        chamberDoor.Unlock()
        chamberDoor.SetOpen(True)
    EndIf
    ObjectReference utilityDoor = Alias_Door_TestChamber_04_Utility.GetReference()
    If utilityDoor
        utilityDoor.Unlock()
        utilityDoor.SetOpen(True)
    EndIf
EndFunction

Function Fragment_Stage_1990_Item_00()
    ObjectReference chamberDoor = Alias_Door_TestChamber_02.GetReference()
    If chamberDoor
        chamberDoor.Unlock()
        chamberDoor.SetOpen(True)
    EndIf
    ObjectReference utilityDoor = Alias_Door_TestChamber_02_Utility.GetReference()
    If utilityDoor
        utilityDoor.Unlock()
        utilityDoor.SetOpen(True)
    EndIf
    SetObjectiveCompleted(1860)
    SetObjectiveDisplayed(1900)
EndFunction

Function Fragment_Stage_1995_Item_00()
    If CassieGreetScene && !CassieGreetScene.IsPlaying()
        CassieGreetScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_2000_Item_00()
    SetObjectiveCompleted(1900)
    If IsStageDone(1900) && !IsStageDone(2100)
        SetStage(2100)
    EndIf
EndFunction

Function Fragment_Stage_2010_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        If playerRef.GetItemCount(DiseaseCureAntibiotics) > 0
            playerRef.RemoveItem(DiseaseCureAntibiotics, 1, False)
        ElseIf playerRef.GetItemCount(DiseaseCureHerbal) > 0
            playerRef.RemoveItem(DiseaseCureHerbal, 1, False)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_2100_Item_00()
    SetObjectiveCompleted(1800)
    SetObjectiveCompleted(1840)
    SetObjectiveCompleted(1850)
    SetObjectiveCompleted(1860)
    SetObjectiveCompleted(1900)
    SetObjectiveDisplayed(2100)
    Int i = 0
    While i < Alias_Doors_TestChamber.GetCount()
        ObjectReference chamberDoor = Alias_Doors_TestChamber.GetAt(i)
        If chamberDoor
            chamberDoor.Unlock()
            chamberDoor.SetOpen(True)
        EndIf
        i += 1
    EndWhile
    ObjectReference researchEncounters = Alias_EM_ResearchEnemies.GetReference()
    If researchEncounters
        researchEncounters.Disable()
    EndIf
EndFunction

Function Fragment_Stage_2200_Item_00()
    SetObjectiveCompleted(2100)
    SetObjectiveDisplayed(2200)
    If BlackburnAmbient_Overseer && !BlackburnAmbient_Overseer.IsPlaying()
        BlackburnAmbient_Overseer.Start()
    EndIf
EndFunction

Function Fragment_Stage_2210_Item_00()
    If Valdez_PuzzleApproach && !Valdez_PuzzleApproach.IsPlaying()
        Valdez_PuzzleApproach.Start()
    EndIf
EndFunction

Function Fragment_Stage_2300_Item_00()
    SetObjectiveCompleted(2200)
    SetObjectiveDisplayed(2300)
    ObjectReference hydraulicControl = Alias_Activator_TempPuzzle.GetReference()
    If hydraulicControl
        hydraulicControl.Enable()
    EndIf
EndFunction

Function Fragment_Stage_2301_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(PuzzleSolution, 1.0)
    EndIf
    If !IsStageDone(2400)
        SetStage(2400)
    EndIf
EndFunction

Function Fragment_Stage_2302_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(PuzzleSolution, 2.0)
    EndIf
    ObjectReference hydraulicMarker = Alias_Marker_Hydraulics_FX.GetReference()
    If hydraulicMarker && ElectricalExplosion
        hydraulicMarker.PlaceAtMe(ElectricalExplosion)
    EndIf
    If !IsStageDone(2400)
        SetStage(2400)
    EndIf
EndFunction

Function Fragment_Stage_2303_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(PuzzleSolution, 3.0)
    EndIf
    ObjectReference hydraulicMarker = Alias_Marker_Hydraulics_FX.GetReference()
    If hydraulicMarker && WaterExplosion
        hydraulicMarker.PlaceAtMe(WaterExplosion)
    EndIf
    If !IsStageDone(2400)
        SetStage(2400)
    EndIf
EndFunction

Function Fragment_Stage_2304_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(PuzzleSolution, 4.0)
    EndIf
    ObjectReference hydraulicMarker = Alias_Marker_Hydraulics_FX.GetReference()
    If hydraulicMarker && WaterExplosion
        hydraulicMarker.PlaceAtMe(WaterExplosion)
    EndIf
    If !IsStageDone(2400)
        SetStage(2400)
    EndIf
EndFunction

Function Fragment_Stage_2310_Item_00()
    SetObjectiveDisplayed(2300)
    ObjectReference hydraulicControl = Alias_Activator_TempPuzzle.GetReference()
    If hydraulicControl
        hydraulicControl.Enable()
    EndIf
EndFunction

Function Fragment_Stage_2400_Item_00()
    SetObjectiveCompleted(2300)
    SetObjectiveDisplayed(2400)
    ObjectReference overseerDoor = Alias_Door_Overseer.GetReference()
    If overseerDoor
        overseerDoor.Unlock()
        overseerDoor.SetOpen(True)
    EndIf
    ObjectReference cardReader = Alias_Cardreader_Overseer.GetReference()
    If cardReader
        cardReader.Disable()
    EndIf
    ObjectReference hydraulicControl = Alias_Activator_TempPuzzle.GetReference()
    If hydraulicControl
        hydraulicControl.Disable()
    EndIf
    If Valdez_PuzzleCommentary && !Valdez_PuzzleCommentary.IsPlaying()
        Valdez_PuzzleCommentary.Start()
    EndIf
EndFunction

Function Fragment_Stage_2470_Item_00()
    ObjectReference blackburnEncounter = Alias_EM_Blackburn.GetReference()
    If blackburnEncounter
        blackburnEncounter.Enable()
    EndIf
    Actor blackburn = Alias_Actor_Blackburn.GetActorReference()
    If blackburn
        blackburn.Enable()
        blackburn.EvaluatePackage()
    EndIf
    If BlackburnOffice_Scene && !BlackburnOffice_Scene.IsPlaying()
        BlackburnOffice_Scene.Start()
    EndIf
EndFunction

Function Fragment_Stage_3000_Item_00()
    SetObjectiveCompleted(2400)
    Actor playerRef = Alias_Player.GetActorReference()
    If !playerRef
        playerRef = Game.GetPlayer()
    EndIf
    If pBS02_MQ05_Catalyst && !pBS02_MQ05_Catalyst.IsRunning() && !pBS02_MQ05_Catalyst.IsCompleted()
        If Catalyst_StartKeyword && playerRef
            Catalyst_StartKeyword.SendStoryEventAndWait(Vault96, playerRef, playerRef)
        EndIf
    EndIf
    If pBS02_MQ05_Catalyst && (pBS02_MQ05_Catalyst.IsRunning() || pBS02_MQ05_Catalyst.IsCompleted()) && !IsStageDone(9000)
        SetStage(9000)
    ElseIf GetCurrentStageID() == 3000
        StartTimer(5.0, 3000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(2400)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef
        playerRef.SetValue(BS01_RahmaniAwayValue, 0.0)
        playerRef.SetValue(ValdezAwayValue, 0.0)
        playerRef.SetValue(ValdezDungeon_AV, 0.0)
        playerRef.SetValue(WoodsPresent_AV, 0.0)
    EndIf
    ObjectReference dungeonEncounters = Alias_EM_ValdezDungeon.GetReference()
    If dungeonEncounters
        dungeonEncounters.Disable()
    EndIf
    ObjectReference vaultDoors = Alias_EM_VaultDoors.GetReference()
    If vaultDoors
        vaultDoors.Disable()
    EndIf
    ObjectReference atriumEncounters = Alias_EM_Atrium_NonQuestCombat.GetReference()
    If atriumEncounters
        atriumEncounters.Disable()
    EndIf
    ObjectReference mainframeEncounters = Alias_EM_MainframeEnemies.GetReference()
    If mainframeEncounters
        mainframeEncounters.Disable()
    EndIf
    ObjectReference reactorEncounters = Alias_EM_ReactorEnemies.GetReference()
    If reactorEncounters
        reactorEncounters.Disable()
    EndIf
    ObjectReference cryoEncounters = Alias_EM_CryoEnemies.GetReference()
    If cryoEncounters
        cryoEncounters.Disable()
    EndIf
    ObjectReference researchEncounters = Alias_EM_ResearchEnemies.GetReference()
    If researchEncounters
        researchEncounters.Disable()
    EndIf
    If Alias_Enemies_Cryobots
        Int cryobotIndex = Alias_Enemies_Cryobots.GetCount() - 1
        While cryobotIndex >= 0
            ObjectReference cryobotRef = Alias_Enemies_Cryobots.GetAt(cryobotIndex)
            If cryobotRef
                Actor cryobotActor = cryobotRef as Actor
                If cryobotActor != None
                    UnregisterForRemoteEvent(cryobotActor, "OnDeath")
                EndIf
                Alias_Enemies_Cryobots.RemoveRef(cryobotRef)
                cryobotRef.Disable()
                cryobotRef.Delete()
            EndIf
            cryobotIndex -= 1
        EndWhile
    EndIf
    Stop()
EndFunction

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    If GetCurrentStageID() == 1400 && Alias_Enemies_Cryobots && Alias_Enemies_Cryobots.GetCount() == 2
        Actor cryobotOne = Alias_Enemies_Cryobots.GetAt(0) as Actor
        Actor cryobotTwo = Alias_Enemies_Cryobots.GetAt(1) as Actor
        If akSender && (akSender == cryobotOne || akSender == cryobotTwo) && cryobotOne && cryobotTwo && cryobotOne.IsDead() && cryobotTwo.IsDead() && !IsStageDone(1500)
            UnregisterForRemoteEvent(cryobotOne, "OnDeath")
            UnregisterForRemoteEvent(cryobotTwo, "OnDeath")
            SetStage(1500)
        EndIf
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 3000 && GetCurrentStageID() == 3000
        Actor playerRef = Alias_Player.GetActorReference()
        If !playerRef
            playerRef = Game.GetPlayer()
        EndIf
        If pBS02_MQ05_Catalyst && !pBS02_MQ05_Catalyst.IsRunning() && !pBS02_MQ05_Catalyst.IsCompleted()
            If Catalyst_StartKeyword && playerRef
                Catalyst_StartKeyword.SendStoryEventAndWait(Vault96, playerRef, playerRef)
            EndIf
        EndIf
        If pBS02_MQ05_Catalyst && (pBS02_MQ05_Catalyst.IsRunning() || pBS02_MQ05_Catalyst.IsCompleted()) && !IsStageDone(9000)
            SetStage(9000)
        ElseIf GetCurrentStageID() == 3000
            StartTimer(5.0, 3000)
        EndIf
    EndIf
EndEvent
