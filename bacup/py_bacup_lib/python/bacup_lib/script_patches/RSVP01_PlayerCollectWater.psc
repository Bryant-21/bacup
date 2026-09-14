Event OnAliasInit()
    If WaterDirty
        AddInventoryEventFilter(WaterDirty)
    EndIf
    If WaterBoiled && WaterBoiled != WaterDirty
        AddInventoryEventFilter(WaterBoiled)
    EndIf
    If RSVP01_REF_Furniture_Well_Pump01
        RegisterForRemoteEvent(RSVP01_REF_Furniture_Well_Pump01, "OnActivate")
    EndIf
    If RSVP01_REF_Furniture_Well_Pump02 && RSVP01_REF_Furniture_Well_Pump02 != RSVP01_REF_Furniture_Well_Pump01
        RegisterForRemoteEvent(RSVP01_REF_Furniture_Well_Pump02, "OnActivate")
    EndIf
    ReconcileBoiledWaterGate()
EndEvent

Event OnPlayerLoadGame()
    RemoveAllInventoryEventFilters()
    If WaterDirty
        AddInventoryEventFilter(WaterDirty)
    EndIf
    If WaterBoiled && WaterBoiled != WaterDirty
        AddInventoryEventFilter(WaterBoiled)
    EndIf
    ReconcileBoiledWaterGate()
EndEvent

Function ReconcileBoiledWaterGate()
    Actor playerRef = GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef && WaterBoiled && RSVP01_Quest_Thirst && RSVP01_Quest_Thirst.IsRunning() && RSVP01_Quest_Thirst.IsStageDone(prereq_Stage_GotBoiledWater) && !RSVP01_Quest_Thirst.IsStageDone(StageToSet_GotBoiledWater)
        If playerRef.GetItemCount(WaterBoiled) == 0
            playerRef.AddItem(WaterBoiled, 1, False)
        EndIf
        If playerRef.GetItemCount(WaterBoiled) > 0
            RSVP01_Quest_Thirst.SetStage(StageToSet_GotBoiledWater)
        EndIf
    EndIf
EndFunction

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || !WaterDirty || !RSVP01_Quest_Thirst
        Return
    EndIf

    Bool allowWellSample = SourceType == 2 || SourceType == 3
    Bool collectingSamples = RSVP01_Quest_Thirst.IsStageDone(prereq_Stage_getDirtyWater) && RSVP01_Quest_Thirst.GetStage() < 700
    If !allowWellSample || !collectingSamples || RSVP01_Quest_Thirst.IsStageDone(StageToSet_GotWellWater)
        Return
    EndIf

    removedItem = WaterDirty
    akActionRef.AddItem(WaterDirty, 1, True)
    If !RSVP01_Quest_Thirst.IsStageDone(StageToSet_GotWellWater)
        RSVP01_Quest_Thirst.SetStage(StageToSet_GotWellWater)
    EndIf
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If aiItemCount <= 0 || !RSVP01_Quest_Thirst
        Return
    EndIf

    If akBaseItem == WaterDirty && removedItem == WaterDirty
        removedItem = None
        If RSVP01_Quest_Thirst.IsStageDone(prereq_Stage_getDirtyWater) && RSVP01_Quest_Thirst.GetStage() < 700 && !RSVP01_Quest_Thirst.IsStageDone(StageToSet_GotWellWater)
            RSVP01_Quest_Thirst.SetStage(StageToSet_GotWellWater)
        EndIf
        Return
    EndIf

    If akBaseItem == WaterBoiled && RSVP01_Quest_Thirst.IsStageDone(prereq_Stage_GotBoiledWater)
        If RSVP01_Quest_Thirst.GetStage() < StageToSet_GotBoiledWater && !RSVP01_Quest_Thirst.IsStageDone(StageToSet_GotBoiledWater)
            RSVP01_Quest_Thirst.SetStage(StageToSet_GotBoiledWater)
        EndIf
        Return
    EndIf

    If akBaseItem != WaterDirty || !RSVP01_Quest_Thirst.IsStageDone(prereq_Stage_getDirtyWater) || RSVP01_Quest_Thirst.GetStage() >= 700
        Return
    EndIf

    Bool allowRiverSample = SourceType == 1 || SourceType == 3
    Bool allowWellSample = SourceType == 2 || SourceType == 3
    Bool fromWellPump = akSourceContainer == RSVP01_REF_Furniture_Well_Pump01 || akSourceContainer == RSVP01_REF_Furniture_Well_Pump02
    If fromWellPump && allowWellSample
        If !RSVP01_Quest_Thirst.IsStageDone(StageToSet_GotWellWater)
            RSVP01_Quest_Thirst.SetStage(StageToSet_GotWellWater)
        EndIf
    ElseIf allowRiverSample && !RSVP01_Quest_Thirst.IsStageDone(StageToSet_GotRiverWater)
        RSVP01_Quest_Thirst.SetStage(StageToSet_GotRiverWater)
    EndIf
EndEvent

Event OnAliasShutdown()
    If RSVP01_REF_Furniture_Well_Pump01
        UnregisterForRemoteEvent(RSVP01_REF_Furniture_Well_Pump01, "OnActivate")
    EndIf
    If RSVP01_REF_Furniture_Well_Pump02 && RSVP01_REF_Furniture_Well_Pump02 != RSVP01_REF_Furniture_Well_Pump01
        UnregisterForRemoteEvent(RSVP01_REF_Furniture_Well_Pump02, "OnActivate")
    EndIf
    removedItem = None
    RemoveAllInventoryEventFilters()
EndEvent
