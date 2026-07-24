Event OnActivate(ObjectReference akActionRef)
    If LC006_PoseidonPlant == None
        Return
    EndIf
    LC006_PoseidonPlantQuestScript questScript = LC006_PoseidonPlant as LC006_PoseidonPlantQuestScript
    If questScript == None
        Return
    EndIf
    Default2StateActivator[] doors = questScript.SecurityDoors
    If doors == None || doors.Length == 0
        Return
    EndIf
    Int clampedStart = StartDoor
    If clampedStart < 0
        clampedStart = 0
    ElseIf clampedStart >= doors.Length
        clampedStart = doors.Length - 1
    EndIf
    Int clampedEnd = EndDoor
    If clampedEnd < 0
        clampedEnd = 0
    ElseIf clampedEnd >= doors.Length
        clampedEnd = doors.Length - 1
    EndIf
    Int lowIndex = clampedStart
    Int highIndex = clampedEnd
    If lowIndex > highIndex
        Int swapTemp = lowIndex
        lowIndex = highIndex
        highIndex = swapTemp
    EndIf
    Int i = lowIndex
    While i <= highIndex
        If doors[i] != None
            doors[i].SetOpen(False)
        EndIf
        i += 1
    EndWhile
EndEvent
