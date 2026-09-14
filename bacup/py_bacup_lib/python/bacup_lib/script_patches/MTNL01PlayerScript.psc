Event OnAliasInit()
    If MTNL01_TrappersNote != None
        AddInventoryEventFilter(MTNL01_TrappersNote)
    EndIf
    If MTNL01_MargieHolotape != None
        AddInventoryEventFilter(MTNL01_MargieHolotape)
    EndIf
    If MTNL01_GourmandsNote != None
        AddInventoryEventFilter(MTNL01_GourmandsNote)
    EndIf
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If akItemReference == None
        Return
    EndIf

    If akBaseItem == MTNL01_TrappersNote && TrappersNote != None && TrappersNote.GetReference() != akItemReference
        TrappersNote.ForceRefTo(akItemReference)
    ElseIf akBaseItem == MTNL01_MargieHolotape && MargieHolotape != None && MargieHolotape.GetReference() != akItemReference
        MargieHolotape.ForceRefTo(akItemReference)
    ElseIf akBaseItem == MTNL01_GourmandsNote && GourmandsNote != None && GourmandsNote.GetReference() != akItemReference
        GourmandsNote.ForceRefTo(akItemReference)
    EndIf
EndEvent

Event OnAliasShutdown()
    RemoveAllInventoryEventFilters()
EndEvent
