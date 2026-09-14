Event OnLocationChange(Location akOldLoc, Location akNewLoc)
    If LocToxicGraftonSteelUndergroundLocation == None || akNewLoc != LocToxicGraftonSteelUndergroundLocation
        Return
    EndIf

    Quest owningQuest = GetOwningQuest()
    If owningQuest != None && owningQuest.IsStageDone(200) && !owningQuest.IsStageDone(300)
        owningQuest.SetStage(300)
    EndIf
EndEvent
