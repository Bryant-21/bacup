Actor Function ActorFromAlias(ReferenceAlias actorAlias)
    If actorAlias == None
        Return None
    EndIf
    Return actorAlias.GetActorReference()
EndFunction

ObjectReference Function PlayerReference()
    Return Alias_Player.GetReference()
EndFunction

Quests:AC_MQ03_HonorBound:QuestScript Function QuestController()
    Return (Self as Quest) as Quests:AC_MQ03_HonorBound:QuestScript
EndFunction

Function StartSceneIfStopped(Scene sceneToStart)
    If sceneToStart != None && !sceneToStart.IsPlaying()
        sceneToStart.Start()
    EndIf
EndFunction

Function EnableAlias(ReferenceAlias targetAlias)
    If targetAlias == None
        Return
    EndIf
    ObjectReference target = targetAlias.GetReference()
    If target != None
        target.Enable()
        Actor targetActor = target as Actor
        If targetActor != None
            targetActor.EvaluatePackage()
        EndIf
    EndIf
EndFunction

Function EnableCollection(RefCollectionAlias targets, Bool attackPlayer = False)
    Int index = 0
    Actor player = ActorFromAlias(Alias_Player)
    While index < targets.GetCount()
        ObjectReference target = targets.GetAt(index)
        If target != None
            target.Enable()
            Actor targetActor = target as Actor
            If targetActor != None
                targetActor.EvaluatePackage()
                If attackPlayer && player != None
                    targetActor.StartCombat(player)
                EndIf
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Function DisableCollection(RefCollectionAlias targets)
    Int index = 0
    While index < targets.GetCount()
        ObjectReference target = targets.GetAt(index)
        If target != None
            target.Disable()
        EndIf
        index += 1
    EndWhile
EndFunction

Function MoveActorToMarker(ReferenceAlias actorAlias, ReferenceAlias markerAlias)
    Actor targetActor = ActorFromAlias(actorAlias)
    ObjectReference marker = markerAlias.GetReference()
    If targetActor != None && marker != None
        targetActor.Enable()
        targetActor.MoveTo(marker)
        targetActor.EvaluatePackage()
    EndIf
EndFunction

Function SetVinPresence(Bool inAtlanticCity)
    Actor vinAppalachia = ActorFromAlias(Alias_Actor_Vin_Appalachia)
    Actor vinAtlantic = ActorFromAlias(Alias_Actor_Vin_AC)
    If inAtlanticCity
        If vinAppalachia != None
            vinAppalachia.SetValue(AC_MQ_Vin_Appalachia_AwayValue, 1.0)
            vinAppalachia.EvaluatePackage()
        EndIf
        If vinAtlantic != None
            vinAtlantic.SetValue(AC_MQ_Vin_AC_AwayValue, 0.0)
            vinAtlantic.Enable()
            vinAtlantic.EvaluatePackage()
        EndIf
    Else
        If vinAppalachia != None
            vinAppalachia.SetValue(AC_MQ_Vin_Appalachia_AwayValue, 0.0)
            vinAppalachia.Enable()
            vinAppalachia.EvaluatePackage()
        EndIf
        If vinAtlantic != None
            vinAtlantic.SetValue(AC_MQ_Vin_AC_AwayValue, 1.0)
            vinAtlantic.EvaluatePackage()
        EndIf
    EndIf
EndFunction

Function GiveWarehouseKeyIfMissing()
    ObjectReference player = PlayerReference()
    If player != None && AC_MQ03_HonorBound_WarehouseKey != None && player.GetItemCount(AC_MQ03_HonorBound_WarehouseKey) < 1
        player.AddItem(AC_MQ03_HonorBound_WarehouseKey, 1, True)
    EndIf
EndFunction

Function CompleteWarehouseEntryObjectives()
    SetObjectiveCompleted(120)
    SetObjectiveCompleted(125)
    SetObjectiveCompleted(130)
    SetObjectiveCompleted(131)
    SetObjectiveCompleted(135)
EndFunction

Function TryFinishWarehouseGuards()
    If IsStageDone(1010) && IsStageDone(1011) && !IsStageDone(1022)
        SetStage(1022)
    EndIf
EndFunction

Function Fragment_Stage_0001_Item_00()
    If !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0002_Item_00()
    If !IsStageDone(800)
        SetStage(800)
    EndIf
EndFunction

Function Fragment_Stage_0003_Item_00()
    If !IsStageDone(900)
        SetStage(900)
    EndIf
EndFunction

Function Fragment_Stage_0004_Item_00()
    If !IsStageDone(1100)
        SetStage(1100)
    EndIf
EndFunction

Function Fragment_Stage_0005_Item_00()
    If !IsStageDone(1200)
        SetStage(1200)
    EndIf
EndFunction

Function Fragment_Stage_0006_Item_00()
    If !IsStageDone(1400)
        SetStage(1400)
    EndIf
EndFunction

Function Fragment_Stage_0007_Item_00()
    If !IsStageDone(1500)
        SetStage(1500)
    EndIf
EndFunction

Function Fragment_Stage_0008_Item_00()
    If !IsStageDone(1700)
        SetStage(1700)
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetVinPresence(False)
    EnableAlias(Alias_Actor_Abbie)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetVinPresence(False)
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
    EnableAlias(Alias_Actor_Vin_Appalachia)
EndFunction

Function Fragment_Stage_0250_Item_00()
    ObjectReference player = PlayerReference()
    If player != None && Caps001 != None && VinCapsAmt > 0
        player.AddItem(Caps001, VinCapsAmt, True)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)
    SetVinPresence(True)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(40)
    SetVinPresence(True)
    MoveActorToMarker(Alias_Actor_Vin_AC, Alias_Marker_Vin_Ambush)
    MoveActorToMarker(Alias_Actor_Sloane, Alias_Marker_Sloane_Ambush)
EndFunction

Function Fragment_Stage_0450_Item_00()
    If OvergrownSFX != None && PlayerReference() != None
        OvergrownSFX.Play(PlayerReference())
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(50)
    EnableAlias(Alias_SpawnCenter_Ambush)
    EnableCollection(Alias_Actors_OvergrownAmbush_Enemies, True)
EndFunction

Function Fragment_Stage_0550_Item_00()
    EnableAlias(Alias_Actor_Sloane)
    StartSceneIfStopped(AC_MQ03_HonorBound_Sloane_Arrival_Scene)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(60)
    EnableAlias(Alias_Actor_Sloane)
    StartSceneIfStopped(AC_MQ03_HonorBound_Sloane_TravelToPostAmbushMarker_Scene)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(70)
    EnableCollection(Alias_Actors_LostMunis)
    StartSceneIfStopped(AC_MQ03_HonorBound_TravelToVanishMarker_Scene)
    Quests:AC_MQ03_HonorBound:QuestScript controller = QuestController()
    If controller != None
        controller.BeginMuniRescue()
    EndIf
EndFunction

Function Fragment_Stage_0725_Item_00()
    EnableAlias(Alias_SpawnCenter_Ambush)
EndFunction

Function Fragment_Stage_0750_Item_00()
    Actor vin = ActorFromAlias(Alias_Actor_Vin_AC)
    Actor sloane = ActorFromAlias(Alias_Actor_Sloane)
    If vin != None
        vin.Disable()
    EndIf
    If sloane != None
        sloane.Disable()
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(70)
    SetObjectiveDisplayed(80)
    MoveActorToMarker(Alias_Actor_Vin_AC, Alias_Marker_Vin_Warehouse)
    MoveActorToMarker(Alias_Actor_Sloane, Alias_Marker_Sloane_Warehouse)
EndFunction

Function Fragment_Stage_0900_Item_00()
    StartSceneIfStopped(AC_MQ03_HonorBound_PreWarehouse_OnApproach_Scene)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(80)
    SetObjectiveDisplayed(120)
    EnableAlias(Alias_Actor_MobBeliever)
    EnableAlias(Alias_Actor_MobDenier)
    EnableAlias(Alias_Book_WarehouseNote)
    EnableAlias(Alias_Activator_BackDoor)
    Quests:AC_MQ03_HonorBound:QuestScript controller = QuestController()
    If controller != None
        controller.BeginBackDoorPuzzle()
    EndIf
EndFunction

Function Fragment_Stage_1005_Item_00()
    Actor player = ActorFromAlias(Alias_Player)
    Actor believer = ActorFromAlias(Alias_Actor_MobBeliever)
    Actor denier = ActorFromAlias(Alias_Actor_MobDenier)
    If believer != None && player != None
        believer.StartCombat(player)
    EndIf
    If denier != None && player != None
        denier.StartCombat(player)
    EndIf
EndFunction

Function Fragment_Stage_1010_Item_00()
    TryFinishWarehouseGuards()
EndFunction

Function Fragment_Stage_1011_Item_00()
    TryFinishWarehouseGuards()
EndFunction

Function Fragment_Stage_1020_Item_00()
    StartSceneIfStopped(AC_MQ03_HonorBound_Mobster_OnApproach_Scene)
EndFunction

Function Fragment_Stage_1022_Item_00()
    SetObjectiveCompleted(125)
    GiveWarehouseKeyIfMissing()
EndFunction

Function Fragment_Stage_1025_Item_00()
    SetObjectiveCompleted(130)
    GiveWarehouseKeyIfMissing()
    StartSceneIfStopped(AC_MQ03_HonorBound_Mobster_TravelToVanishMarker_Scene)
EndFunction

Function Fragment_Stage_1026_Item_00()
    SetObjectiveCompleted(131)
    GiveWarehouseKeyIfMissing()
EndFunction

Function Fragment_Stage_1030_Item_00()
    Actor believer = ActorFromAlias(Alias_Actor_MobBeliever)
    Actor denier = ActorFromAlias(Alias_Actor_MobDenier)
    If believer != None
        believer.Disable()
    EndIf
    If denier != None
        denier.Disable()
    EndIf
EndFunction

Function Fragment_Stage_1050_Item_00()
    SetObjectiveCompleted(135)
    ObjectReference backDoor = Alias_Door_Warehouse_Back.GetReference()
    If backDoor != None
        backDoor.BlockActivation(False)
        backDoor.Lock(False)
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    CompleteWarehouseEntryObjectives()
    SetObjectiveDisplayed(140)
EndFunction

Function Fragment_Stage_1175_Item_00()
    StartSceneIfStopped(AC_MQ03_HonorBound_VinSloane_PostWarehouse_OnApproach_Scene)
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveCompleted(140)
    SetObjectiveDisplayed(150)
EndFunction

Function Fragment_Stage_1300_Item_00()
    SetObjectiveCompleted(150)
    SetObjectiveDisplayed(160)
    StartSceneIfStopped(AC_MQ03_HonorBound_VinSloane_TravelToChemLabVanishMarker_Scene)
EndFunction

Function Fragment_Stage_1350_Item_00()
    MoveActorToMarker(Alias_Actor_Vin_AC, Alias_Marker_Vin_ChemLab)
    MoveActorToMarker(Alias_Actor_Sloane, Alias_Marker_Sloane_ChemLab)
    StartSceneIfStopped(AC_MQ03_HonorBound_TravelToChemLab_Scene)
EndFunction

Function Fragment_Stage_1400_Item_00()
    SetObjectiveCompleted(160)
    SetObjectiveDisplayed(170)
    EnableAlias(Alias_Actor_Vin_AC)
    EnableAlias(Alias_Actor_Sloane)
    StartSceneIfStopped(AC_MQ03_HonorBound_DiscussPipes_Scene)
EndFunction

Function Fragment_Stage_1425_Item_00()
    StartSceneIfStopped(AC_MQ03_HonorBound_Sloane_ExitForever_Scene)
EndFunction

Function Fragment_Stage_1450_Item_00()
    Actor sloane = ActorFromAlias(Alias_Actor_Sloane)
    If sloane != None
        sloane.Disable()
    EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
    SetObjectiveCompleted(170)
    SetObjectiveDisplayed(180)
    Quests:AC_MQ03_HonorBound:QuestScript controller = QuestController()
    If controller != None
        controller.BeginValvePuzzle()
    EndIf
EndFunction

Function Fragment_Stage_1600_Item_00()
    SetObjectiveCompleted(180)
    SetObjectiveDisplayed(190)
    ObjectReference entrance = Alias_Static_ChemLabEntrance.GetReference()
    ObjectReference exitRef = Alias_Static_ChemLabExit.GetReference()
    ObjectReference explosionMarker = Alias_Marker_WaterExplosion.GetReference()
    If entrance != None
        entrance.Disable()
    EndIf
    If exitRef != None
        exitRef.Enable()
    EndIf
    EnableCollection(Alias_MovableStatics_WaterFX)
    If explosionMarker != None && ChemLabEntranceExplosion != None
        explosionMarker.PlaceAtMe(ChemLabEntranceExplosion)
    EndIf
    ObjectReference player = PlayerReference()
    If player != None && ScreenShakeSpell != None
        ScreenShakeSpell.Cast(player, player)
    EndIf
    ObjectReference chemLabDoor = Alias_Door_ChemLab.GetReference()
    If chemLabDoor != None
        chemLabDoor.BlockActivation(False)
        chemLabDoor.Lock(False)
    EndIf
    StartSceneIfStopped(AC_MQ03_HonorBound_Vin_FloodedChemLab_Scene)
EndFunction

Function Fragment_Stage_1700_Item_00()
    SetObjectiveCompleted(190)
    SetObjectiveDisplayed(200)
    EnableCollection(Alias_Actors_ChemLabEnemies, True)
EndFunction

Function Fragment_Stage_1800_Item_00()
    SetObjectiveCompleted(200)
    SetObjectiveDisplayed(210)
    EnableAlias(Alias_Actor_Vin_AC)
EndFunction

Function Fragment_Stage_1900_Item_00()
    SetObjectiveCompleted(210)
    SetObjectiveDisplayed(220)
    StartSceneIfStopped(AC_MQ03_HonorBound_Vin_TravelToGenesRoom_Scene)
EndFunction

Function Fragment_Stage_1950_Item_00()
    MoveActorToMarker(Alias_Actor_Vin_AC, Alias_Marker_Vin_GenesRoom)
EndFunction

Function Fragment_Stage_2000_Item_00()
    SetObjectiveCompleted(220)
    SetObjectiveDisplayed(230)
    Actor gene = ActorFromAlias(Alias_Actor_Gene)
    If gene != None
        gene.Enable()
        gene.StopCombat()
        gene.AddToFaction(CaptiveFaction)
        gene.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_2075_Item_00()
    MoveActorToMarker(Alias_Actor_Vin_AC, Alias_Marker_Vin_GenesRoom)
EndFunction

Function Fragment_Stage_2100_Item_00()
    SetObjectiveCompleted(230)
    SetObjectiveCompleted(240)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(240)
    SetVinPresence(False)
    ObjectReference player = PlayerReference()
    If AC_MQ04_Sins_StartKeyword != None && player != None
        AC_MQ04_Sins_StartKeyword.SendStoryEvent(None, player, player)
    EndIf
EndFunction
