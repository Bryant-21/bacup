Function Fragment_Phase_02_Begin()
    ObjectReference repairFurniture = RepairFurnitureRef.GetReference()
    If repairFurniture != None
        repairFurniture.Disable()
    EndIf

    If W05_RE_ObjectAF01_RobotFaction != None && W05_RE_ObjectAF01_HumanFaction != None
        W05_RE_ObjectAF01_RobotFaction.SetEnemy(W05_RE_ObjectAF01_HumanFaction)
    EndIf

    Actor protectronRef = DisProtectron.GetActorReference()
    If protectronRef != None
        protectronRef.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Phase_02_End()
    If W05_RE_ObjectAF01_RobotFaction != None && W05_RE_ObjectAF01_HumanFaction != None
        W05_RE_ObjectAF01_RobotFaction.SetAlly(W05_RE_ObjectAF01_HumanFaction)
    EndIf
EndFunction
