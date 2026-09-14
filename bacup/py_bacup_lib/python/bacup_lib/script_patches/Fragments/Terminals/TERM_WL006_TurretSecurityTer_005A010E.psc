Function PowerLinkedTurrets(ObjectReference akTerminalRef, Keyword akLinkKeyword)
    ObjectReference[] linkedRefs = akTerminalRef.GetLinkedRefArray(akLinkKeyword)
    Int i = 0
    While i < linkedRefs.Length
        Actor turret = linkedRefs[i] as Actor
        If turret != None
            turret.Enable(False)
            turret.SetUnconscious(False)
            turret.EvaluatePackage(False)
        EndIf
        i += 1
    EndWhile
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    PowerLinkedTurrets(akTerminalRef, Turret01Keyword)
    PowerLinkedTurrets(akTerminalRef, Turret02Keyword)
    ObjectReference soundMarker = akTerminalRef.GetLinkedRef(PowerUpSoundKeyword)
    If soundMarker != None && PowerUpSound != None
        PowerUpSound.Play(soundMarker)
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && DivertTurretPowerStartedActorValue != None
        playerRef.SetValue(DivertTurretPowerStartedActorValue, 1.0)
    EndIf
EndFunction
