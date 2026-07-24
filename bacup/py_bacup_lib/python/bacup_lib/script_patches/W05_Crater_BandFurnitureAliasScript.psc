Event OnActivate(ObjectReference akActionRef)
    If BandFurnitures == None
        Return
    EndIf

    iFurnituresInUse = 0
    Int i = 0
    Int count = BandFurnitures.Length
    While i < count
        If BandFurnitures[i] != None
            ObjectReference furnitureRef = BandFurnitures[i].GetReference()
            If furnitureRef != None && furnitureRef.IsFurnitureInUse()
                iFurnituresInUse += 1
            EndIf
        EndIf
        i += 1
    EndWhile

    If iFurnituresInUse == 0 && W05_Crater_Talent != None
        W05_Crater_Talent.SetValue(Utility.RandomInt(0, 1) as Float)
    EndIf
EndEvent
