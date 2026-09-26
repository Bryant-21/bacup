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
Function ReconcileTerminalState()
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || MoM02ATerminalValue == None
        Return
    EndIf
    If IsStageDone(50)
        playerRef.SetValue(MoM02ATerminalValue, CONST_MoM02AValue_ReadyForFabrication)
    ElseIf IsStageDone(40)
        playerRef.SetValue(MoM02ATerminalValue, CONST_MoM02AValue_LocatedTargets)
    ElseIf IsStageDone(30)
        playerRef.SetValue(MoM02ATerminalValue, CONST_MoM02AValue_ReadyForTargets)
    EndIf
EndFunction
