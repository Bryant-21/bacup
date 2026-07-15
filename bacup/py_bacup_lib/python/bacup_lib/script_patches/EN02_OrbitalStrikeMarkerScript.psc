Int remainingDelayCount

Event OnLoad()
    remainingDelayCount = fireDelayCount
    If remainingDelayCount <= 0 || fireDelay <= 0.0
        FireStrike()
    Else
        PlayWarningBeep()
        StartTimer(fireDelay, 0)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 0
        remainingDelayCount = remainingDelayCount - 1
        If remainingDelayCount <= 0
            FireStrike()
        Else
            PlayWarningBeep()
            StartTimer(fireDelay, 0)
        EndIf
    EndIf
EndEvent

Function PlayWarningBeep()
    If WPNGrenadeOrbitalStrikeBeep != None
        WPNGrenadeOrbitalStrikeBeep.Play(Self)
    EndIf
EndFunction

Function FireStrike()
    ObjectReference shooterRef
    If EN02_OrbitalStrikeShooter != None
        shooterRef = PlaceAtMe(EN02_OrbitalStrikeShooter)
    EndIf
    If shooterRef == None && EN02_OrbitalStrikeFailedMessage != None
        EN02_OrbitalStrikeFailedMessage.Show()
    EndIf
    Delete()
EndFunction
