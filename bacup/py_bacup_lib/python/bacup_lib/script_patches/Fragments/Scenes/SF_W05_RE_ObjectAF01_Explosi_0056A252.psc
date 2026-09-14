Function Fragment_Phase_02_End()
    Actor protectronRef = DisProtectron.GetActorReference()
    If protectronRef != None && W05_RE_ObjectAF01_Protectron_Destruct != None
        W05_RE_ObjectAF01_Protectron_Destruct.Cast(protectronRef, protectronRef)
    EndIf
EndFunction

Function Fragment_Phase_05_End()
    ObjectReference repairFurniture = RepairFurnitureRef.GetReference()
    If repairFurniture != None
        repairFurniture.Disable()
    EndIf
EndFunction
