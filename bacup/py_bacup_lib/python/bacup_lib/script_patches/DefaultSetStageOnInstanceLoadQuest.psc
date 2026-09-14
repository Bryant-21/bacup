Event OnQuestInit()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnLocationChange")
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
        CheckPlayerInstanceLocation(playerRef.GetCurrentLocation())
    EndIf
EndEvent

Event OnQuestShutdown()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnLocationChange")
        UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
EndEvent

Event Actor.OnLocationChange(Actor akSender, Location akOldLoc, Location akNewLoc)
    If akSender == Game.GetPlayer()
        CheckPlayerInstanceLocation(akNewLoc)
    EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        CheckPlayerInstanceLocation(akSender.GetCurrentLocation())
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        CheckPlayerInstanceLocation(playerRef.GetCurrentLocation())
    EndIf
EndEvent

Function CheckPlayerInstanceLocation(Location playerLocation)
    If playerLocation == None
        Return
    EndIf

    Int index = 0
    While InstancedLocationData && index < InstancedLocationData.Length
        LocationDatum locationData = InstancedLocationData[index]
        If locationData.TargetLocation == playerLocation && locationData.StageToSet >= 0 && !IsStageDone(locationData.StageToSet)
            If (locationData.PreReqStage < 0 || IsStageDone(locationData.PreReqStage)) && (locationData.ShutdownStage < 0 || GetStage() < locationData.ShutdownStage)
                SetStage(locationData.StageToSet)
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction
