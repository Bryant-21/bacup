Function SetLinkedTurretsEnabled(ObjectReference akTerminalRef, Bool abEnabled)
    ObjectReference[] linkedRefs = akTerminalRef.GetLinkedRefArray(LinkTerminalTurret)
    Int i = 0
    While i < linkedRefs.Length
        Actor turret = linkedRefs[i] as Actor
        If turret != None
            turret.StopCombat()
            turret.SetUnconscious(!abEnabled)
            turret.EvaluatePackage(False)
        EndIf
        i = i + 1
    EndWhile
EndFunction

Function RemoveLinkedTurretTargetingRestrictions(ObjectReference akTerminalRef)
    ObjectReference[] linkedRefs = akTerminalRef.GetLinkedRefArray(LinkTerminalTurret)
    Actor player = Game.GetPlayer()
    Int i = 0
    While i < linkedRefs.Length
        Actor turret = linkedRefs[i] as Actor
        If turret != None
            turret.SetUnconscious(False)
            turret.StartCombat(player)
        EndIf
        i = i + 1
    EndWhile
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    SetLinkedTurretsEnabled(akTerminalRef, False)
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    SetLinkedTurretsEnabled(akTerminalRef, True)
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    RemoveLinkedTurretTargetingRestrictions(akTerminalRef)
EndFunction
