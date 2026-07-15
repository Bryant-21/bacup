Event OnLoad()
    If strikeHeight != 0.0
        SetPosition(GetPositionX(), GetPositionY(), GetPositionZ() + strikeHeight)
    EndIf
    If FXOrbitalStrikeEntry3D != None
        FXOrbitalStrikeEntry3D.Play(Self)
    EndIf
    If EN02_OrbitalStrikeWeapon != None
        EN02_OrbitalStrikeWeapon.Fire(Self)
    EndIf
    Delete()
EndEvent
