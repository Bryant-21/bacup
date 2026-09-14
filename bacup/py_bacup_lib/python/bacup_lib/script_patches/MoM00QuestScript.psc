Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID != CONST_MoM00_FoundBody
        Return
    EndIf

    ObjectReference corpse = MoM00Corpse.GetReference()
    If corpse == None
        Return
    EndIf

    ObjectReference damagedHolotape = MoM00Holotape.GetReference()
    If damagedHolotape != None && damagedHolotape.GetContainer() != corpse
        corpse.AddItem(damagedHolotape, 1, True)
    EndIf

    ObjectReference wornVeil = MoM00WornVeil.GetReference()
    If wornVeil != None && wornVeil.GetContainer() != corpse
        corpse.AddItem(wornVeil, 1, True)
    EndIf
EndEvent
