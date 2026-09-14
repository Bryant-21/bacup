Event OnQuestInit()
    ReconcileRuntimeRegistrations()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        ReconcileRuntimeRegistrations()
    EndIf
EndEvent

Event OnQuestShutdown()
    ClearRuntimeRegistrations()
EndEvent

Function ClearRuntimeRegistrations()
    Actor player = Game.GetPlayer()
    If player != None
        UnregisterForRemoteEvent(player, "OnLocationChange")
        UnregisterForRemoteEvent(player, "OnPlayerLoadGame")
    EndIf
    ObjectReference doorTrigger = BrodysRoomDoorTrigger.GetReference()
    If doorTrigger != None
        UnregisterForRemoteEvent(doorTrigger, "OnTriggerEnter")
    EndIf
    ObjectReference roomTrigger = BrodysRoomInteriorTrigger.GetReference()
    If roomTrigger != None
        UnregisterForRemoteEvent(roomTrigger, "OnTriggerEnter")
    EndIf
EndFunction

Function ReconcileRuntimeRegistrations()
    ClearRuntimeRegistrations()
    If !IsRunning() || IsCompleted()
        Return
    EndIf

    Actor player = Game.GetPlayer()
    If player != None
        RegisterForRemoteEvent(player, "OnLocationChange")
        RegisterForRemoteEvent(player, "OnPlayerLoadGame")
    EndIf
    ObjectReference doorTrigger = BrodysRoomDoorTrigger.GetReference()
    If doorTrigger != None
        RegisterForRemoteEvent(doorTrigger, "OnTriggerEnter")
    EndIf
    ObjectReference roomTrigger = BrodysRoomInteriorTrigger.GetReference()
    If roomTrigger != None
        RegisterForRemoteEvent(roomTrigger, "OnTriggerEnter")
    EndIf
EndFunction

Event Actor.OnLocationChange(Actor akSender, Location akOldLoc, Location akNewLoc)
    If akSender != Game.GetPlayer() || !IsRunning() || IsCompleted() || !IsStageDone(CONST_CHECKPOINT_MoM03_InfiltratePleasantValley) || IsStageDone(CONST_MoM03_InfiltratedPleasantValley)
        Return
    EndIf

    If akNewLoc == PleasantValleyCabinsLocation.GetLocation() || akNewLoc == PleasantValleySkiResortLocation.GetLocation() || akNewLoc == TopOfTheWorldLocation.GetLocation()
        SetStage(CONST_MoM03_InfiltratedPleasantValley)
    EndIf
EndEvent

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || !IsRunning() || IsCompleted()
        Return
    EndIf

    If akSender == BrodysRoomDoorTrigger.GetReference() && IsStageDone(CONST_MoM03_FoundClueToBrodysRoom) && !IsStageDone(CONST_MoM03_EnteredDoorTrigger)
        SetStage(CONST_MoM03_EnteredDoorTrigger)
    ElseIf akSender == BrodysRoomInteriorTrigger.GetReference() && IsStageDone(CONST_MoM03_EnteredDoorTrigger) && !IsStageDone(CONST_MoM03_EnteredRoomTrigger)
        SetStage(CONST_MoM03_EnteredRoomTrigger)
    EndIf
EndEvent
