Function Fragment_Stage_0100_Item_00()
    ObjectReference akPlayerRef = Alias_owningPlayer.GetReference()
    if akPlayerRef == None
        return
    endif

    if Alias_Paige.GetReference() != None
        akPlayerRef.SetValue(W05_PaigeIsInFoundation, 1.0)
    else
        akPlayerRef.SetValue(W05_PaigeIsInFoundation, 0.0)
    endif

    if Alias_Penny.GetReference() != None
        akPlayerRef.SetValue(W05_PennyIsInFoundation, 1.0)
    else
        akPlayerRef.SetValue(W05_PennyIsInFoundation, 0.0)
    endif

    if Alias_Jen.GetReference() != None
        akPlayerRef.SetValue(W05_JenIsInFoundation, 1.0)
    else
        akPlayerRef.SetValue(W05_JenIsInFoundation, 0.0)
    endif
EndFunction
