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

; Stage 10 (RunOnStart) carries no bound fragment, and the authored path out of it
; -- scene SFZ04_Recycle_MaintenanceMessage into the Red Rocket terminal menu -- has no
; working substitute after conversion, so the daily would sit with no displayed objective
; whenever DefaultDailyQuestScript does not receive its story event.
Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 10 && !IsStageDone(100)
        SetStage(100)
    EndIf
EndEvent
