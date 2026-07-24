Event OnQuestInit()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnLocationChange")
        CheckPlayerLocation(playerRef.GetCurrentLocation())
    EndIf
EndEvent

Event Actor.OnLocationChange(Actor akSender, Location akOldLoc, Location akNewLoc)
    If akSender == Game.GetPlayer()
        CheckPlayerLocation(akNewLoc)
    EndIf
EndEvent

Function CheckPlayerLocation(Location playerLocation)
    If playerLocation == None
        Return
    EndIf

    Int index = 0
    While EnterInstancedLocationStages && index < EnterInstancedLocationStages.Length
        EnterInstancedLocationStage stageData = EnterInstancedLocationStages[index]
        Location targetLocation = stageData.TargetLocation
        If targetLocation == None && stageData.TargetLocationAlias != None
            targetLocation = stageData.TargetLocationAlias.GetLocation()
        EndIf

        If targetLocation == playerLocation && !IsStageDone(stageData.StageToSet)
            If (stageData.PrereqStage < 0 || IsStageDone(stageData.PrereqStage)) && (stageData.TurnOffStage < 0 || GetStage() < stageData.TurnOffStage)
                SetStage(stageData.StageToSet)
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction
