Actor Function ResolveLocalPlayer()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && Alias_Player != None && Alias_Player.GetReference() != playerRef
        Alias_Player.ForceRefTo(playerRef)
    EndIf
    Return playerRef
EndFunction

ObjectReference Function ResolveAliasReference(ReferenceAlias targetAlias)
    If targetAlias != None
        Return targetAlias.GetReference()
    EndIf
    Return None
EndFunction

Function SetAliasEnabled(ReferenceAlias targetAlias, Bool shouldEnable)
    ObjectReference targetRef = ResolveAliasReference(targetAlias)
    If targetRef != None
        If shouldEnable
            targetRef.Enable()
        Else
            targetRef.Disable()
        EndIf
    EndIf
EndFunction

Function SetCollectionEnabled(RefCollectionAlias targetCollection, Bool shouldEnable)
    Int index = 0
    While targetCollection != None && index < targetCollection.GetCount()
        ObjectReference targetRef = targetCollection.GetAt(index)
        If targetRef != None
            If shouldEnable
                targetRef.Enable()
            Else
                targetRef.Disable()
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Function MoveActorAliasTo(ReferenceAlias actorAlias, ReferenceAlias markerAlias)
    Actor actorRef = None
    ObjectReference markerRef = ResolveAliasReference(markerAlias)
    If actorAlias != None
        actorRef = actorAlias.GetActorReference()
    EndIf
    If actorRef != None && markerRef != None
        actorRef.Enable()
        actorRef.MoveTo(markerRef)
        actorRef.EvaluatePackage()
    EndIf
EndFunction

Function EvaluateActorAlias(ReferenceAlias actorAlias)
    Actor actorRef = None
    If actorAlias != None
        actorRef = actorAlias.GetActorReference()
    EndIf
    If actorRef != None
        actorRef.EvaluatePackage()
    EndIf
EndFunction

Function SetPlayerValue(ActorValue valueToSet, Float value)
    Actor playerRef = ResolveLocalPlayer()
    If playerRef != None && valueToSet != None
        playerRef.SetValue(valueToSet, value)
    EndIf
EndFunction

Function StartSceneIfIdle(Scene sceneToStart)
    If sceneToStart != None && !sceneToStart.IsPlaying()
        sceneToStart.Start()
    EndIf
EndFunction

Bool Function AttemptPenanceHandoff()
    Actor playerRef = ResolveLocalPlayer()
    Bool accepted = False
    If BS02_MQ01_Penance != None
        accepted = BS02_MQ01_Penance.IsRunning() || BS02_MQ01_Penance.IsCompleted()
    EndIf
    If !accepted && playerRef != None && BS02_MQ01_Penance != None && BS02_MQ01_Penance_StartKeyword != None
        accepted = BS02_MQ01_Penance_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        If !accepted
            accepted = BS02_MQ01_Penance.IsRunning() || BS02_MQ01_Penance.IsCompleted()
        EndIf
    EndIf
    If accepted
        If playerRef != None
            ReferenceAlias nextPlayerAlias = BS02_MQ01_Penance.GetAlias(0) as ReferenceAlias
            If nextPlayerAlias != None && nextPlayerAlias.GetReference() != playerRef
                nextPlayerAlias.ForceRefTo(playerRef)
            EndIf
        EndIf
        CancelTimer(9000)
        Stop()
        Return True
    EndIf
    CancelTimer(9000)
    StartTimer(5.0, 9000)
    Return False
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != 9000 || !IsStageDone(9000)
        Return
    EndIf
    AttemptPenanceHandoff()
EndEvent

Event OnQuestShutdown()
    CancelTimer(9000)
EndEvent

Function Fragment_Stage_0520_Item_00()
    SetAliasEnabled(Alias_Static_BombB, True)
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0250_Item_00()
    SetPlayerValue(BS01_ValdezAwayValue, 1.0)
    SetAliasEnabled(Alias_Marker_BreachCrisisDuringEnableMarker, True)
    StartSceneIfIdle(BS01_MQ08_Defense_BarricadeTravel_Scene)
EndFunction

Function Fragment_Stage_0415_Item_00()
    SetAliasEnabled(Alias_EnableMarker_AmbientRobots, False)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(50)
    SetAliasEnabled(Alias_EnableMarker_AmbientSuperMutants, True)
    SetCollectionEnabled(Alias_Actors_AmbientEnemies_EndRoom, True)
    If !IsStageDone(510)
        SetAliasEnabled(Alias_Static_BombA, False)
    EndIf
    If !IsStageDone(520)
        SetAliasEnabled(Alias_Static_BombB, False)
    EndIf
    If !IsStageDone(530)
        SetAliasEnabled(Alias_Static_BombC, False)
    EndIf
    If !IsStageDone(540)
        SetAliasEnabled(Alias_Static_BombD, False)
    EndIf
EndFunction

Function Fragment_Stage_0540_Item_00()
    SetAliasEnabled(Alias_Static_BombD, True)
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0002_Item_00()
    If IsStageDone(1)
        If !IsStageDone(510)
            SetStage(510)
        EndIf
        If !IsStageDone(520)
            SetStage(520)
        EndIf
        If !IsStageDone(530)
            SetStage(530)
        EndIf
        If !IsStageDone(540)
            SetStage(540)
        EndIf
        If !IsStageDone(600)
            SetStage(600)
        EndIf
        If !IsStageDone(700)
            SetStage(700)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(70)
    StartSceneIfIdle(BS01_MQ08_Defense_BombDetonationQuip_Scene)
EndFunction

Function Fragment_Stage_0480_Item_00()
    StartSceneIfIdle(BS01_MQ08_Defense_BonusConvBiAGaveWeapons_Scene)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0100_Item_00()
    ResolveLocalPlayer()
    SetPlayerValue(BS01_ShinAwayValue, 1.0)
    SetPlayerValue(BS01_RahmaniAwayValue, 1.0)
    SetPlayerValue(BS01_MQ08_Defense_ShinPAAwayValue, 0.0)
    SetPlayerValue(BS01_MQ08_Defense_RahmaniPAAwayValue, 0.0)
    SetPlayerValue(BS01_ValdezAwayValue, 0.0)
    SetPlayerValue(BS01_MQ08_Defense_ProtectronWorkerAwayValue, 1.0)
    SetAliasEnabled(Alias_EnableMarker_BreachBefore, True)
    SetAliasEnabled(Alias_EnableMarker_BreachAfter, False)
    SetAliasEnabled(Alias_Marker_BreachCrisisEnableMarker, True)
    SetAliasEnabled(Alias_Marker_BreachCrisisDisableMarker, False)
    SetAliasEnabled(Alias_Marker_BreachCrisisDuringEnableMarker, False)
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0510_Item_00()
    SetAliasEnabled(Alias_Static_BombA, True)
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0001_Item_00()
    Actor playerRef = ResolveLocalPlayer()
    ObjectReference skipMarker = ResolveAliasReference(Alias_DEBUG_Marker_BombRoomSkipMarker)
    If playerRef != None && skipMarker != None
        playerRef.MoveTo(skipMarker)
        Int bombsRequired = BS01_MQ08_Defense_BombsMax_Global.GetValueInt()
        Int bombsMissing = bombsRequired - playerRef.GetItemCount(BS01_MQ08_Defense_Bomb_MiscItem)
        If bombsMissing > 0
            playerRef.AddItem(BS01_MQ08_Defense_Bomb_MiscItem, bombsMissing, True)
        EndIf
        If !IsStageDone(500)
            SetStage(500)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
    Actor playerRef = ResolveLocalPlayer()
    If playerRef != None
        Int bombsRequired = BS01_MQ08_Defense_BombsMax_Global.GetValueInt()
        Int bombsMissing = bombsRequired - playerRef.GetItemCount(BS01_MQ08_Defense_Bomb_MiscItem)
        If bombsMissing > 0
            playerRef.AddItem(BS01_MQ08_Defense_Bomb_MiscItem, bombsMissing, True)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(70)
    SetObjectiveDisplayed(80)
    SetObjectiveDisplayed(85)
    SetAliasEnabled(Alias_Activator_ElevatorButton, True)
    SetAliasEnabled(Alias_Activator_TunnelRocksActivator, True)
    StartSceneIfIdle(BS01_MQ08_Defense_BackToSurfaceQuip_Scene)
    StartSceneIfIdle(BS01_MQ08_Defense_GoUpstairsTravel_Scene)
EndFunction

Function Fragment_Stage_9000_Item_00()
    ResolveLocalPlayer()
    CompleteAllObjectives()
    CompleteQuest()
    SetPlayerValue(BS01_ShinAwayValue, 0.0)
    SetPlayerValue(BS01_RahmaniAwayValue, 0.0)
    SetPlayerValue(BS01_MQ08_Defense_ShinPAAwayValue, 1.0)
    SetPlayerValue(BS01_MQ08_Defense_RahmaniPAAwayValue, 1.0)
    SetPlayerValue(BS01_ValdezAwayValue, 0.0)
    SetPlayerValue(BS01_MQ08_Defense_ProtectronWorkerAwayValue, 0.0)
    AttemptPenanceHandoff()
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
    ObjectReference upperDoor = ResolveAliasReference(Alias_Door_SubstructureDoor)
    ObjectReference lowerDoor = ResolveAliasReference(Alias_Door_SubstructureDoorLower)
    If upperDoor != None
        upperDoor.Enable()
        upperDoor.Lock(False)
    EndIf
    If lowerDoor != None
        lowerDoor.Enable()
        lowerDoor.Lock(False)
    EndIf
    StartSceneIfIdle(BS01_MQ08_Defense_GoDownstairsTravel_Scene)
EndFunction

Function Fragment_Stage_0003_Item_00()
    ResolveLocalPlayer()
EndFunction

Function Fragment_Stage_0475_Item_00()
    EvaluateActorAlias(Alias_Actor_Valdez_Dungeon)
    EvaluateActorAlias(Alias_Actor_Shin_Dungeon)
    EvaluateActorAlias(Alias_Actor_Rahmani_Dungeon)
EndFunction

Function Fragment_Stage_0850_Item_00()
    SetObjectiveCompleted(80)
    SetPlayerValue(BS01_ShinAwayValue, 0.0)
    SetPlayerValue(BS01_RahmaniAwayValue, 0.0)
    SetPlayerValue(BS01_MQ08_Defense_ShinPAAwayValue, 1.0)
    SetPlayerValue(BS01_MQ08_Defense_RahmaniPAAwayValue, 1.0)
    SetPlayerValue(BS01_ValdezAwayValue, 0.0)
    SetPlayerValue(BS01_MQ08_Defense_ProtectronWorkerAwayValue, 0.0)
    SetAliasEnabled(Alias_EnableMarker_BreachBefore, False)
    SetAliasEnabled(Alias_EnableMarker_BreachAfter, True)
    SetAliasEnabled(Alias_Marker_BreachCrisisEnableMarker, False)
    SetAliasEnabled(Alias_Marker_BreachCrisisDisableMarker, True)
    SetAliasEnabled(Alias_Marker_BreachCrisisDuringEnableMarker, False)
    MoveActorAliasTo(Alias_Actor_Valdez_Base, Alias_Marker_Valdez_EndXMarker)
    MoveActorAliasTo(Alias_Actor_Shin_NoPABase, Alias_Marker_Shin_EndXMarker)
    MoveActorAliasTo(Alias_Actor_Rahmani_NoPABase, Alias_Marker_Rahmani_EndXMarker)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0825_Item_00()
    SetObjectiveCompleted(85)
EndFunction

Function Fragment_Stage_0410_Item_00()
    SetAliasEnabled(Alias_EnableMarker_AmbientRobots, True)
    SetAliasEnabled(Alias_Marker_WaterRoomSpawnCenter, True)
    SetCollectionEnabled(Alias_Actors_GenericEWSEnemies, True)
EndFunction

Function Fragment_Stage_0150_Item_00()
    MoveActorAliasTo(Alias_Actor_Valdez_Base, Alias_Marker_Valdez_InitialXMarker)
    MoveActorAliasTo(Alias_Actor_Shin_Base, Alias_Marker_Shin_InitialXMarker)
    MoveActorAliasTo(Alias_Actor_Rahmani_Base, Alias_Marker_Rahmani_InitialXMarker)
EndFunction

Function Fragment_Stage_0450_Item_00()
    EvaluateActorAlias(Alias_Actor_Valdez_Dungeon)
    EvaluateActorAlias(Alias_Actor_Shin_Dungeon)
    EvaluateActorAlias(Alias_Actor_Rahmani_Dungeon)
EndFunction

Function Fragment_Stage_0420_Item_00()
    SetAliasEnabled(Alias_EnableMarker_AmbientBugs, True)
    SetAliasEnabled(Alias_Marker_PitRoomSpawnCenter, True)
    SetCollectionEnabled(Alias_Actors_AmbientEnemies_PitRoom, True)
EndFunction

Function Fragment_Stage_0425_Item_00()
    SetAliasEnabled(Alias_EnableMarker_AmbientBugs, False)
EndFunction

Function Fragment_Stage_0530_Item_00()
    SetAliasEnabled(Alias_Static_BombC, True)
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0600_Item_00()
    Actor playerRef = ResolveLocalPlayer()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(60)
    If playerRef != None && playerRef.GetItemCount(BS01_MQ08_Defense_Bomb_MiscItem) > 0
        playerRef.RemoveItem(BS01_MQ08_Defense_Bomb_MiscItem, -1, True)
    EndIf
    StartSceneIfIdle(BS01_MQ08_Defense_SafeDistance_Scene)
EndFunction

Function Fragment_Stage_0830_Item_00()
    SetObjectiveFailed(85)
EndFunction
