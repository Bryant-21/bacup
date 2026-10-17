Event OnQuestInit()
    WatchWorkshopBuilds()
EndEvent

Event OnQuestShutdown()
    UnregisterForAllEvents()
EndEvent

; Called by COMP_InitialQuestScript when a lite ally is hired: shows that ally's
; "build it at your C.A.M.P." objective through its NeedToBuild stage.
Bool Function RequestCampObject(Keyword akCampObjectKeyword)
    If !akCampObjectKeyword
        Return False
    EndIf
    Bool found = False
    Int index = 0
    While CampObjectStageData && index < CampObjectStageData.Length
        CampObjectStageDatum datum = CampObjectStageData[index]
        If datum.CampObject && datum.CampObject.HasKeyword(akCampObjectKeyword)
            found = True
            If !IsStageDone(datum.Stage_Built) && !IsStageDone(datum.Stage_NeedToBuild)
                SetStage(datum.Stage_NeedToBuild)
            EndIf
        EndIf
        index += 1
    EndWhile
    WatchWorkshopBuilds()
    Return found
EndFunction

Function MarkBuilt(Form akBuiltBase)
    Int index = 0
    While akBuiltBase && CampObjectStageData && index < CampObjectStageData.Length
        CampObjectStageDatum datum = CampObjectStageData[index]
        If datum.CampObject == akBuiltBase && IsStageDone(datum.Stage_NeedToBuild) && !IsStageDone(datum.Stage_Built)
            SetStage(datum.Stage_Built)
        EndIf
        index += 1
    EndWhile
EndFunction

; Stage 9000 completes the quest, and it is RunOnce: only finish once every ally's object is built.
Function CompleteIfAllBuilt()
    Int index = 0
    While CampObjectStageData && index < CampObjectStageData.Length
        If !IsStageDone(CampObjectStageData[index].Stage_Built)
            Return
        EndIf
        index += 1
    EndWhile
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function WatchWorkshopBuilds()
    WorkshopParentScript workshopParent = Game.GetFormFromFile(0x0002058E, "Fallout4.esm") as WorkshopParentScript
    If workshopParent
        RegisterForCustomEvent(workshopParent, "WorkshopEnterMenu")
    EndIf
EndFunction

Event WorkshopParentScript.WorkshopEnterMenu(WorkshopParentScript akSender, Var[] akArgs)
    If akArgs && akArgs.Length > 1
        ObjectReference workshop = akArgs[1] as ObjectReference
        If workshop
            RegisterForRemoteEvent(workshop, "OnWorkshopObjectPlaced")
        EndIf
    EndIf
EndEvent

Event ObjectReference.OnWorkshopObjectPlaced(ObjectReference akSender, ObjectReference akReference)
    If akReference
        MarkBuilt(akReference.GetBaseObject())
    EndIf
EndEvent
