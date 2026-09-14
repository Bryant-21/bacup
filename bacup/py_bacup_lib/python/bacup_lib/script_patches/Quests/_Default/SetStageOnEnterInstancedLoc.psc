Event OnQuestInit()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnLocationChange")
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
        CheckPlayerLocation(playerRef.GetCurrentLocation())
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
        CheckPlayerLocation(akNewLoc)
    EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        CheckPlayerLocation(akSender.GetCurrentLocation())
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        CheckPlayerLocation(playerRef.GetCurrentLocation())
    EndIf
EndEvent

Function CheckPlayerLocation(Location playerLocation)
    If playerLocation == None
        Return
    EndIf

    Int index = 0
    While EnterInstancedLocationStages != None && index < EnterInstancedLocationStages.Length
        EnterInstancedLocationStage stageData = EnterInstancedLocationStages[index]
        Location targetLocation = ResolveTargetLocation(stageData)

        If targetLocation == playerLocation && stageData.StageToSet >= 0 && !IsStageDone(stageData.StageToSet)
            If (stageData.PrereqStage < 0 || IsStageDone(stageData.PrereqStage)) && (stageData.TurnOffStage < 0 || GetStage() < stageData.TurnOffStage)
                SetStage(stageData.StageToSet)
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Location Function ResolveTargetLocation(EnterInstancedLocationStage stageData)
    If stageData.InstancedRefAlias != None
        ObjectReference instancedRef = stageData.InstancedRefAlias.GetReference()
        If instancedRef != None
            Location instancedLocation = instancedRef.GetCurrentLocation()
            If instancedLocation != None
                Return instancedLocation
            EndIf
        EndIf
    EndIf

    If stageData.TargetLocation != None
        Return stageData.TargetLocation
    EndIf
    If stageData.TargetLocationAlias != None
        Return stageData.TargetLocationAlias.GetLocation()
    EndIf
    Return None
EndFunction
