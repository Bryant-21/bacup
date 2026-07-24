Event OnLocationChange(Location akOldLoc, Location akNewLoc)
    If LocToxicGraftonSteelUndergroundLocation == None || akNewLoc != LocToxicGraftonSteelUndergroundLocation
        Return
    EndIf

    Quest owningQuest = GetOwningQuest()
    If owningQuest != None && owningQuest.IsStageDone(200) && !owningQuest.IsStageDone(310)
        owningQuest.SetStage(310)
    EndIf
EndEvent
