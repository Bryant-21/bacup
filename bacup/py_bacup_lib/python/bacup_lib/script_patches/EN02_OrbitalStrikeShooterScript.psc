Event OnLoad()
    If strikeHeight != 0.0
        SetPosition(GetPositionX(), GetPositionY(), GetPositionZ() + strikeHeight)
    EndIf
    If FXOrbitalStrikeEntry3D != None
        FXOrbitalStrikeEntry3D.Play(Self)
    EndIf
    ; Self is a placed marker, not an actor — Weapon.Fire must be given an
    ; explicit ammo or the engine resolves it off a null actor process.
    If EN02_OrbitalStrikeWeapon != None
        Ammo strikeAmmo = EN02_OrbitalStrikeWeapon.GetAmmo()
        If strikeAmmo != None
            EN02_OrbitalStrikeWeapon.Fire(Self, strikeAmmo)
        EndIf
    EndIf
    Delete()
EndEvent
