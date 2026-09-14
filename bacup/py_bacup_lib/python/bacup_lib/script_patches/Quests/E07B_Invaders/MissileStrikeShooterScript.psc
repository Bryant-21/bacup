Event OnLoad()
    If strikeHeight != 0.0
        SetPosition(GetPositionX(), GetPositionY(), GetPositionZ() + strikeHeight)
    EndIf
    If FXOrbitalStrikeEntry3D != None
        FXOrbitalStrikeEntry3D.Play(Self)
    EndIf
    ; Self is a placed marker, not an actor — Weapon.Fire must be given an
    ; explicit ammo or the engine resolves it off a null actor process.
    If E07B_Invaders_MissileStrikeWeapon != None
        Ammo strikeAmmo = E07B_Invaders_MissileStrikeWeapon.GetAmmo()
        If strikeAmmo != None
            E07B_Invaders_MissileStrikeWeapon.Fire(Self, strikeAmmo)
        EndIf
    EndIf
    Delete()
EndEvent
