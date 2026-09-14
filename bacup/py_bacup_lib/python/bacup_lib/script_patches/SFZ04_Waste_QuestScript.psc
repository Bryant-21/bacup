Function AssignCoreToProtectron(RefCollectionAlias akSpawnedProtectrons)
    If akSpawnedProtectrons == None || akSpawnedProtectrons.GetCount() == 0
        Return
    EndIf
    If CoresInHolding == None || CoresInHolding.GetCount() == 0 || CoresInWorld == None
        Return
    EndIf

    ObjectReference protectronRef = akSpawnedProtectrons.GetAt(0)
    ObjectReference coreRef = CoresInHolding.GetAt(0)
    If protectronRef == None || coreRef == None
        Return
    EndIf

    CoresInHolding.RemoveRef(coreRef)
    If CoresInWorld.Find(coreRef) < 0
        CoresInWorld.AddRef(coreRef)
    EndIf
    protectronRef.AddItem(coreRef, 1, True)
EndFunction
