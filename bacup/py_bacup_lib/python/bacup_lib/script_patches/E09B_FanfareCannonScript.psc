Event OnActivate(ObjectReference akActionRef)
    If ConfettiExplosion != None
        PlaceAtMe(ConfettiExplosion as Form, 1, False, False, True)
    EndIf

    If FireworksWeapons == None
        Return
    EndIf

    Int weaponIndex = 0
    While weaponIndex < FireworksWeapons.Length
        ; Self is an activator, not an actor — Weapon.Fire must be given an
        ; explicit ammo or the engine resolves it off a null actor process.
        If FireworksWeapons[weaponIndex] != None
            Ammo fireworkAmmo = FireworksWeapons[weaponIndex].GetAmmo()
            If fireworkAmmo != None
                FireworksWeapons[weaponIndex].Fire(Self, fireworkAmmo)
            EndIf
        EndIf
        weaponIndex += 1
    EndWhile
EndEvent
