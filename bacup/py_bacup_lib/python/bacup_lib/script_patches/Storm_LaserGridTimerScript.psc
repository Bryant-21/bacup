Event OnInit()
    bCurrentlyAllowingAccess = bStartsOpen
    ObjectReference[] chain = Self.GetLinkedRefChain()
    If chain != None
        laserGrids = New storm_weaponizedlasergridscript[chain.Length]
        Int i = 0
        While i < chain.Length
            laserGrids[i] = chain[i] as storm_weaponizedlasergridscript
            i += 1
        EndWhile
    EndIf
    ApplyGridState(bCurrentlyAllowingAccess)
    If fTimerInterval > 0.0
        StartTimer(fTimerInterval, 1)
    EndIf
EndEvent

Function ApplyGridState(Bool bAllowAccess)
    If laserGrids == None
        Return
    EndIf
    Int i = 0
    While i < laserGrids.Length
        If laserGrids[i] != None
            laserGrids[i].UpdateClientAnimationStateValue(bAllowAccess)
        EndIf
        i += 1
    EndWhile
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1
        bCurrentlyAllowingAccess = !bCurrentlyAllowingAccess
        ApplyGridState(bCurrentlyAllowingAccess)
        If fTimerInterval > 0.0
            StartTimer(fTimerInterval, 1)
        EndIf
    EndIf
EndEvent
