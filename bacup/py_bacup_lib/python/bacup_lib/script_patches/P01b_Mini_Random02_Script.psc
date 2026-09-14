Event OnQuestInit()
    RegisterForDenDistance()
EndEvent

Function RegisterForDenDistance()
    ObjectReference playerRef = alias_Player.GetReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    ObjectReference denMarker = alias_DenMarker.GetReference()
    If playerRef != None && denMarker != None && !IsStageDone(500)
        UnregisterForDistanceEvents(playerRef, denMarker)
        If denMarker.Is3DLoaded() && playerRef.GetDistance(denMarker) <= Distance
            SetStage(500)
        Else
            RegisterForDistanceLessThanEvent(playerRef, denMarker, Distance)
        EndIf
    EndIf
EndFunction

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
    ObjectReference playerRef = alias_Player.GetReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    ObjectReference denMarker = alias_DenMarker.GetReference()
    If !IsStageDone(500) && ((akObj1 == playerRef && akObj2 == denMarker) || (akObj2 == playerRef && akObj1 == denMarker))
        UnregisterForDistanceEvents(playerRef, denMarker)
        SetStage(500)
    EndIf
EndEvent

Event OnQuestShutdown()
    ObjectReference playerRef = alias_Player.GetReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    ObjectReference denMarker = alias_DenMarker.GetReference()
    If playerRef != None && denMarker != None
        UnregisterForDistanceEvents(playerRef, denMarker)
    EndIf
EndEvent
