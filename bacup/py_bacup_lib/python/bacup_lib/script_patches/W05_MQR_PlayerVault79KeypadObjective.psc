Event OnLocationChange(Location akOldLoc, Location akNewLoc)
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || InstancedLocationAlias == None
        Return
    EndIf

    Location targetLocation = InstancedLocationAlias.GetLocation()
    If targetLocation == None || akNewLoc != targetLocation
        Return
    EndIf

    Int currentStage = owningQuest.GetStage()
    If currentStage >= PreReqStage && currentStage < EndOnStage
        If !owningQuest.IsObjectiveDisplayed(KeypadObjective)
            owningQuest.SetObjectiveDisplayed(KeypadObjective)
        EndIf
    EndIf
EndEvent
