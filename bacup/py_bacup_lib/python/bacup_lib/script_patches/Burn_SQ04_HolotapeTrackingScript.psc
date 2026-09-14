Quest Function TrackingQuest()
    If MasterQuest == None
        MasterQuest = GetOwningQuest()
    EndIf
    Return MasterQuest
EndFunction

Function SetStageForHolotape(Form akHolotape)
    If akHolotape == None || HolotapeStageTracking == None
        Return
    EndIf
    Quest owner = TrackingQuest()
    If owner == None
        Return
    EndIf
    Int index = 0
    While index < HolotapeStageTracking.Length
        If HolotapeStageTracking[index].DirtyLaundryHolotapes == akHolotape
            Int stageToSet = HolotapeStageTracking[index].StageToSet
            If stageToSet > 0 && !owner.IsStageDone(stageToSet)
                owner.SetStage(stageToSet)
            EndIf
            Return
        EndIf
        index += 1
    EndWhile
EndFunction

; FO4 inventory events are opt-in: OnItemAdded is never dispatched to a script
; that has not registered an inventory event filter first.
Function RegisterHolotapeFilters()
    RemoveAllInventoryEventFilters()
    If HolotapeStageTracking == None
        Return
    EndIf
    Int index = 0
    While index < HolotapeStageTracking.Length
        Holotape tracked = HolotapeStageTracking[index].DirtyLaundryHolotapes
        If tracked != None
            AddInventoryEventFilter(tracked)
        EndIf
        index += 1
    EndWhile
EndFunction

Event OnAliasInit()
    MasterQuest = GetOwningQuest()
    RegisterHolotapeFilters()
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If !SetOnEnd
        SetStageForHolotape(akBaseItem)
    EndIf
EndEvent

Event OnItemEquipped(Form akBaseObject, ObjectReference akReference)
    If SetOnEnd
        SetStageForHolotape(akBaseObject)
    EndIf
EndEvent
