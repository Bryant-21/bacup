Event OnInit()
    InitializeItemCreation()
EndEvent

Event OnAliasInit()
    InitializeItemCreation()
EndEvent

Function InitializeItemCreation()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnLocationChange")
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
    EnsureItemInContainer()
EndFunction

Event OnLoad()
    EnsureItemInContainer()
EndEvent

Event Actor.OnLocationChange(Actor akSender, Location akOldLoc, Location akNewLoc)
    If akSender == Game.GetPlayer() && akNewLoc == InstancedLocation
        EnsureItemInContainer()
    EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        EnsureItemInContainer()
    EndIf
EndEvent

Event OnAliasShutdown()
    UnregisterForAllRemoteEvents()
EndEvent

Function EnsureItemInContainer()
    Quest owningQuest = GetOwningQuest()
    If creatingItem || owningQuest == None || !owningQuest.IsRunning() || ItemtoAdd == None || InstanceOwner == None || ItemDestinationAlias == None
        Return
    EndIf
    ObjectReference containerRef = GetReference()
    Actor playerRef = InstanceOwner.GetActorReference()
    If containerRef == None || playerRef == None || playerRef != Game.GetPlayer()
        Return
    EndIf
    If InstancedLocation != None && (containerRef.GetCurrentLocation() != InstancedLocation || playerRef.GetCurrentLocation() != InstancedLocation)
        Return
    EndIf
    If TurnOffAV != None && playerRef.GetValue(TurnOffAV) >= TurnOffAVValue
        Return
    EndIf
    If ItemDestinationAlias.GetReference() != None
        Return
    EndIf
    creatingItem = True
    ObjectReference newRef = containerRef.PlaceAtMe(ItemtoAdd, 1, True)
    If newRef != None
        containerRef.AddItem(newRef, 1, True)
        ItemDestinationAlias.ForceRefTo(newRef)
    EndIf
    creatingItem = False
EndFunction
