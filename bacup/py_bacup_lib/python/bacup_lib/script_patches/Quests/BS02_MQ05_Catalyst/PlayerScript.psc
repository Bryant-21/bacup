Event OnAliasInit()
    InitializePlayerLocationTracking()
EndEvent

Event OnPlayerLoadGame()
    InitializePlayerLocationTracking()
EndEvent

Event Actor.OnLocationChange(Actor akSender, Location akOldLoc, Location akNewLoc)
    If akSender != Game.GetPlayer()
        Return
    EndIf

    ReconcileFortAtlasEntry(akNewLoc)
    ReconcileCleanup(akOldLoc, akNewLoc)
EndEvent

Event OnAliasShutdown()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnLocationChange")
    EndIf
EndEvent

Function InitializePlayerLocationTracking()
    Actor playerRef = GetActorReference()
    If playerRef != Game.GetPlayer()
        Return
    EndIf

    myQI = GetOwningQuest()
    UnregisterForRemoteEvent(playerRef, "OnLocationChange")
    RegisterForRemoteEvent(playerRef, "OnLocationChange")
    ReconcileFortAtlasEntry(playerRef.GetCurrentLocation())
EndFunction

Function ReconcileFortAtlasEntry(Location currentLocation)
    Actor playerRef = GetActorReference()
    Quest owningQuest = myQI
    Location fortAtlas = None

    If owningQuest == None
        owningQuest = GetOwningQuest()
        myQI = owningQuest
    EndIf
    If Loc_FortAtlas != None
        fortAtlas = Loc_FortAtlas.GetLocation()
    EndIf

    If playerRef == Game.GetPlayer() && owningQuest != None && fortAtlas != None && currentLocation == fortAtlas && owningQuest.IsStageDone(1475) && !owningQuest.IsStageDone(20)
        owningQuest.SetStage(20)
    EndIf
EndFunction

Function ReconcileCleanup(Location oldLocation, Location newLocation)
    Actor playerRef = GetActorReference()
    Quest owningQuest = myQI
    Location fortAtlas = None

    If owningQuest == None
        owningQuest = GetOwningQuest()
        myQI = owningQuest
    EndIf
    If Loc_FortAtlas != None
        fortAtlas = Loc_FortAtlas.GetLocation()
    EndIf

    If playerRef == Game.GetPlayer() && owningQuest != None && fortAtlas != None && oldLocation == fortAtlas && newLocation != fortAtlas && owningQuest.IsStageDone(CompletionStage) && !owningQuest.IsStageDone(CleanupStage)
        owningQuest.SetStage(CleanupStage)
    EndIf
EndFunction
