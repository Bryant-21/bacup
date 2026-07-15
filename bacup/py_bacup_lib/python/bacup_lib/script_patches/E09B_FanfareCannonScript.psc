Event OnActivate(ObjectReference akActionRef)
    If ConfettiExplosion != None
        PlaceAtMe(ConfettiExplosion as Form, 1, False, False, True)
    EndIf

    If FireworksWeapons == None
        Return
    EndIf

    Int weaponIndex = 0
    While weaponIndex < FireworksWeapons.Length
        If FireworksWeapons[weaponIndex] != None
            FireworksWeapons[weaponIndex].Fire(Self)
        EndIf
        weaponIndex += 1
    EndWhile
EndEvent
