Event OnLocationChange(Location akOldLoc, Location akNewLoc)
    If myQI == None
        myQI = GetOwningQuest()
    EndIf
    If myDungeonLoc == None && ATLASDungeonLocation != None
        myDungeonLoc = ATLASDungeonLocation.GetLocation()
    EndIf
    If mySpawnCenter == None && Alias_UltraciteFight_SpawnCenterMarker != None
        mySpawnCenter = Alias_UltraciteFight_SpawnCenterMarker.GetReference()
    EndIf
    If myQI != None && myDungeonLoc != None
        If akOldLoc == myDungeonLoc && akNewLoc != myDungeonLoc && myQI.IsStageDone(CompletionStage) && !myQI.IsStageDone(CleanupStage)
            myQI.SetStage(CleanupStage)
        ElseIf akNewLoc == myDungeonLoc && myQI.IsStageDone(RobotFightStage) && !myQI.IsStageDone(RobotFightEndedStage)
            If mySpawnCenter != None
                mySpawnCenter.Enable()
                mySpawnCenter.Activate(GetReference())
            EndIf
        EndIf
    EndIf
EndEvent
