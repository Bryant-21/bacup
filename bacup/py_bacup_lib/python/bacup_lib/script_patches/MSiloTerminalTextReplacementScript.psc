Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTarget)
    RefreshTerminal(akTarget)
EndEvent

Function RefreshTerminal(ObjectReference akTarget)
    If akTarget == None || MSilo == None
        Return
    EndIf
    If replaceControlValues || replaceControlRobotFabricatorValues
        (MSilo as MSiloQuestScript_Control).UpdateTerminal(akTarget)
    EndIf
    If replaceOperationsValues
        (MSilo as MSiloQuestScript_Operations).UpdateTerminal(akTarget)
    EndIf
    If replaceStorageValues
        (MSilo as MSiloQuestScript_Storage).UpdateTerminal(akTarget)
    EndIf
EndFunction
