Event OnQuestInit()
    PlayerRef = Game.GetPlayer()
    ObjectReference liteAlly
    If Alias_LiteAlly
        liteAlly = Alias_LiteAlly.GetReference()
    EndIf
    If liteAlly && OPTIONAL_ShutDownQuestWhenLiteAllyUnloads
        RegisterForRemoteEvent(liteAlly, "OnUnload")
    EndIf
    If CampObjectAvailable()
        WatchWorkshopBuilds()
    EndIf
EndEvent

Event OnQuestShutdown()
    UnregisterForAllEvents()
EndEvent

; The full intros (Beckett 2000, Sofia 3000) display the CAMP objective from a stage
; that shares its number; displaying it is what unlocks the companion's camp object.
Event OnStageSet(Int auiStageID, Int auiItemID)
    If Objective_Build_Object > 0 && auiStageID == Objective_Build_Object
        MakeCampObjectAvailable()
    EndIf
EndEvent

; Called from the lite-ally hire scene's closing phase.
Function AcquireAlly()
    If !IsRunning()
        Return
    EndIf
    MakeCampObjectAvailable()
    Bool tracked = RequestBuildObjective()
    If !tracked && Objective_Build_Object >= 0 && !IsObjectiveDisplayed(Objective_Build_Object)
        SetObjectiveDisplayed(Objective_Build_Object)
    EndIf
    If Alias_LiteAlly && Alias_LiteAlly.GetReference()
        ; Completion is what stops CompanionLiteAllyEffectScript offering this hire again.
        CompleteQuest()
    EndIf
EndFunction

Function MakeCampObjectAvailable()
    Actor player = Game.GetPlayer()
    If player && CompanionCampObjectAvailableAV && player.GetValue(CompanionCampObjectAvailableAV) < iCompanionCampObjectAvailable
        player.SetValue(CompanionCampObjectAvailableAV, iCompanionCampObjectAvailable as Float)
    EndIf
    WatchWorkshopBuilds()
EndFunction

Bool Function CampObjectAvailable()
    Actor player = Game.GetPlayer()
    Return player && CompanionCampObjectAvailableAV && player.GetValue(CompanionCampObjectAvailableAV) > iCompanionCampObjectAvailableAV_Default
EndFunction

; The shared objectives quest owns the lite allies' "build it at your C.A.M.P." objective,
; so it outlives this dialogue-only quest.
Bool Function RequestBuildObjective()
    COMP_BuildCampObjectObjectivesScript objectives = COMP_Quest_Intro_Lite_BuildCampObjectObjectives as COMP_BuildCampObjectObjectivesScript
    If !objectives || !Keyword_CampObject
        Return False
    EndIf
    If !objectives.IsRunning() && COMP_BuildCampObjectObjectivesStart
        COMP_BuildCampObjectObjectivesStart.SendStoryEventAndWait(None, Game.GetPlayer())
    EndIf
    If !objectives.IsRunning()
        Return False
    EndIf
    Return objectives.RequestCampObject(Keyword_CampObject)
EndFunction

Function WatchWorkshopBuilds()
    If Stage_Object_Built <= 0 || !Keyword_CampObject || IsStageDone(Stage_Object_Built)
        Return
    EndIf
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
    If !akReference || !Keyword_CampObject || !akReference.HasKeyword(Keyword_CampObject)
        Return
    EndIf
    If Stage_Object_Built > 0 && IsRunning() && !IsStageDone(Stage_Object_Built) && IsStageDone(Objective_Build_Object)
        SetStage(Stage_Object_Built)
    EndIf
EndEvent

Event ObjectReference.OnUnload(ObjectReference akSender)
    If Alias_LiteAlly && akSender == Alias_LiteAlly.GetReference() && OPTIONAL_ShutDownQuestWhenLiteAllyUnloads
        Stop()
    EndIf
EndEvent
