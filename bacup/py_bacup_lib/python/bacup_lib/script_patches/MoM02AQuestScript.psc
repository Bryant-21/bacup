Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID != CONST_CHECKPOINT_MoM02A_LocateTargets
        Return
    EndIf

    ObjectReference gasContainer = GasCanisterContainer.GetReference()
    ObjectReference gasCanister = MoM02AGasCanister.GetReference()
    If gasContainer != None && gasCanister != None && gasCanister.GetContainer() != gasContainer
        gasContainer.AddItem(gasCanister, 1, True)
    EndIf

    ObjectReference stealthContainer = StealthBoyContainer.GetReference()
    ObjectReference stealthBoyRef = MoM02AStealthBoy.GetReference()
    If stealthContainer != None && stealthBoyRef != None && stealthBoyRef.GetContainer() != stealthContainer
        stealthContainer.AddItem(stealthBoyRef, 1, True)
    EndIf
    ObjectReference note = StealthBoyNote.GetReference()
    If stealthContainer != None && note != None && note.GetContainer() != stealthContainer
        stealthContainer.AddItem(note, 1, True)
    EndIf
EndEvent
