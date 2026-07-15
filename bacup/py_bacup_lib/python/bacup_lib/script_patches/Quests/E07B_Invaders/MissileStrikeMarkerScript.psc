Event OnLoad()
    If WPNGrenadeOrbitalStrikeBeep != None
        WPNGrenadeOrbitalStrikeBeep.Play(Self)
    EndIf

    If fireDelay > 0.0
        StartTimer(fireDelay, 0)
    Else
        FireStrike()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 0
        FireStrike()
    EndIf
EndEvent

Function FireStrike()
    If E07B_Invaders_MissileStrikeShooter != None
        PlaceAtMe(E07B_Invaders_MissileStrikeShooter)
    EndIf
EndFunction
