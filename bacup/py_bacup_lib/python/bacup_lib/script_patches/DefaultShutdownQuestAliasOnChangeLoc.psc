Event OnLocationChange(Location akOldLoc, Location akNewLoc)
    If !CheckOnLeave || !ContainsLocation(akOldLoc) || ContainsLocation(akNewLoc)
        Return
    EndIf
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || IsPreventedStage(owningQuest.GetStage())
        Return
    EndIf
    If ShutdownStage > 0 && !owningQuest.IsStageDone(ShutdownStage)
        owningQuest.SetStage(ShutdownStage)
    EndIf
EndEvent

Bool Function ContainsLocation(Location targetLocation)
    If targetLocation == None
        Return False
    EndIf
    Int index = 0
    While LocationsToCheck != None && index < LocationsToCheck.Length
        If LocationsToCheck[index] == targetLocation
            Return True
        EndIf
        index += 1
    EndWhile
    Return False
EndFunction

Bool Function IsPreventedStage(Int aiStage)
    bFoundStage = False
    Int index = 0
    While PreventShutdownStages != None && index < PreventShutdownStages.Length
        If PreventShutdownStages[index] == aiStage
            bFoundStage = True
            Return True
        EndIf
        index += 1
    EndWhile
    Return False
EndFunction
