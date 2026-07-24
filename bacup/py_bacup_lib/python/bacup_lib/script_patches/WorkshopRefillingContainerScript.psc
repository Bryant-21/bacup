Event OnInit()
    If Variable01 != None
        Self.SetValue(Variable01, Utility.GetCurrentGameTime())
    EndIf
    If WorkshopRefillingGrenade != None
        AddInventoryEventFilter(WorkshopRefillingGrenade)
    EndIf
    If DestroyWithLinkedRef
        ObjectReference linkedRef = Self.GetLinkedRef()
        If linkedRef != None
            RegisterForRemoteEvent(linkedRef, "OnDestructionStageChanged")
        EndIf
    EndIf
EndEvent

Event OnItemRemoved(Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    MaybeRefill()
EndEvent

Function MaybeRefill()
    If WorkshopRefillingGrenade == None || Variable01 == None || MinGrenadeCount <= 0
        Return
    EndIf
    If Self.GetItemCount(WorkshopRefillingGrenade) >= MinGrenadeCount
        Return
    EndIf
    If Utility.GetCurrentGameTime() - Self.GetValue(Variable01) < ResetInventoryTimeDays
        Return
    EndIf
    Self.AddItem(WorkshopRefillingGrenade, MinGrenadeCount, true)
    Self.SetValue(Variable01, Utility.GetCurrentGameTime())
EndFunction

Event ObjectReference.OnDestructionStageChanged(ObjectReference akSender, int aiOldStage, int aiCurrentStage)
    If akSender != Self.GetLinkedRef()
        Return
    EndIf
    If aiCurrentStage > aiOldStage
        Self.Delete()
    EndIf
EndEvent
