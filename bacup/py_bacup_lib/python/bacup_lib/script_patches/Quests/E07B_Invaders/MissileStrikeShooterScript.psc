Event OnLoad()
    If strikeHeight != 0.0
        SetPosition(GetPositionX(), GetPositionY(), GetPositionZ() + strikeHeight)
    EndIf
    If FXOrbitalStrikeEntry3D != None
        FXOrbitalStrikeEntry3D.Play(Self)
    EndIf
    If E07B_Invaders_MissileStrikeWeapon != None
        E07B_Invaders_MissileStrikeWeapon.Fire(Self)
    EndIf
    Delete()
EndEvent
