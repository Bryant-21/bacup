Event OnQuestInit()
    Parent.OnQuestInit()
    RegisterLocalWorkshopEvents()
    ReconcileCampProgress()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    Actor playerRef = Game.GetPlayer()
    Int index = 0
    While playerRef != None && CheckpointAV != None && CheckpointStages != None && index < CheckpointStages.Length
        If auiStageID == CheckpointStages[index].checkpointValue && playerRef.GetValue(CheckpointAV) < auiStageID
            playerRef.SetValue(CheckpointAV, auiStageID)
        EndIf
        index += 1
    EndWhile
    RegisterLocalWorkshopEvents()
    ReconcileCampProgress()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        RegisterLocalWorkshopEvents()
        ReconcileCampProgress()
    EndIf
EndEvent

Event OnQuestShutdown()
    UnregisterForAllRemoteEvents()
    UnregisterForAllCustomEvents()
EndEvent

Function RegisterLocalWorkshopEvents()
    If !IsRunning() || IsCompleted()
        Return
    EndIf
    RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
    WorkshopParentScript workshopParent = Game.GetFormFromFile(0x02058E, "Fallout4.esm") as WorkshopParentScript
    If workshopParent == None
        Return
    EndIf
    RegisterForCustomEvent(workshopParent, "WorkshopObjectBuilt")
    Int index = 0
    While workshopParent.Workshops != None && index < workshopParent.Workshops.Length
        WorkshopScript workshopRef = workshopParent.Workshops[index]
        If IsLocalCamp(workshopRef)
            RegisterForRemoteEvent(workshopRef, "OnWorkshopMode")
        EndIf
        index += 1
    EndWhile
EndFunction

Bool Function IsLocalCamp(WorkshopScript akWorkshop)
    If akWorkshop == None || !akWorkshop.OwnedByPlayer || akWorkshop.IsDisabled() || akWorkshop.GetWorldSpace() == None
        Return False
    EndIf
    Form baseObject = akWorkshop.GetBaseObject()
    Return baseObject == Game.GetFormFromFile(0x12D065, "SeventySix.esm") || baseObject == Game.GetFormFromFile(0xFFF064, "B21_TalesFromAppalachia.esm")
EndFunction

Event WorkshopParentScript.WorkshopObjectBuilt(WorkshopParentScript akSender, Var[] akArgs)
    If akSender != Game.GetFormFromFile(0x02058E, "Fallout4.esm") || akArgs == None || akArgs.Length < 2
        Return
    EndIf
    WorkshopScript workshopRef = akArgs[1] as WorkshopScript
    If IsLocalCamp(workshopRef)
        ReconcileCampProgress()
        RecordCampConstruction(workshopRef, akArgs[0] as ObjectReference)
    EndIf
EndEvent

Event ObjectReference.OnWorkshopMode(ObjectReference akSender, Bool aStart)
    If IsLocalCamp(akSender as WorkshopScript)
        ReconcileCampProgress()
    EndIf
EndEvent

Function ReconcileCampProgress()
    B21CampProgressDirty = True
    If B21CampProgressBusy || !IsRunning() || IsCompleted() || !IsStageDone(2000)
        Return
    EndIf
    B21CampProgressBusy = True
    WorkshopParentScript workshopParent = Game.GetFormFromFile(0x02058E, "Fallout4.esm") as WorkshopParentScript
    While B21CampProgressDirty && workshopParent != None && IsRunning() && !IsCompleted()
        B21CampProgressDirty = False
        Int index = 0
        While workshopParent.Workshops != None && index < workshopParent.Workshops.Length
            WorkshopScript workshopRef = workshopParent.Workshops[index]
            If IsLocalCamp(workshopRef)
                RegisterForRemoteEvent(workshopRef, "OnWorkshopMode")
                If !IsStageDone(CampStage)
                    SetStage(CampStage)
                EndIf
                ObjectReference[] builtObjects = workshopRef.GetLinkedRefChildren(workshopParent.WorkshopItemKeyword)
                Int objectIndex = 0
                While builtObjects != None && objectIndex < builtObjects.Length
                    RecordCampConstruction(workshopRef, builtObjects[objectIndex])
                    objectIndex += 1
                EndWhile
            EndIf
            index += 1
        EndWhile
    EndWhile
    B21CampProgressBusy = False
EndFunction

Function RecordCampConstruction(WorkshopScript akWorkshop, ObjectReference akObject)
    If !IsRunning() || IsCompleted() || !IsStageDone(CampStage) || akObject == None || !IsLocalCamp(akWorkshop)
        Return
    EndIf
    WorkshopParentScript workshopParent = akWorkshop.WorkshopParent
    If workshopParent == None || akObject.IsDeleted() || akObject.IsDisabled() || akObject.GetLinkedRef(workshopParent.WorkshopItemKeyword) != akWorkshop
        Return
    EndIf
    Keyword localCooking = Game.GetFormFromFile(0x102152, "Fallout4.esm") as Keyword
    If IsObjectiveDisplayed(CookingStationStage) && !IsStageDone(CookingStationStage) && (akObject.HasKeyword(WorkbenchCooking) || akObject.HasKeyword(localCooking))
        SetStage(CookingStationStage)
    EndIf
    If IsObjectiveDisplayed(StashboxStage) && !IsStageDone(StashboxStage) && akObject.HasKeyword(StashBoxKeyword)
        SetStage(StashboxStage)
    EndIf
    If IsObjectiveDisplayed(GeneratorStage) && !IsStageDone(GeneratorStage) && workshopParent.PowerGenerated != None && akObject.GetValue(workshopParent.PowerGenerated) > 0.0
        SetStage(GeneratorStage)
    EndIf
EndFunction
