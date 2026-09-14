Function Fragment_Terminal_05(ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If LCP_BoSZ01 != None
        LCP_BoSZ01.SetValue(1.0)
    EndIf
    If playerRef != None && pBoSz01_PlayerKACacheDepot != None
        playerRef.SetValue(pBoSz01_PlayerKACacheDepot, 1.0)
    EndIf
    If pBoSZ01 != None && pBoSZ01.IsRunning() && !pBoSZ01.IsStageDone(150)
        pBoSZ01.SetStage(150)
    EndIf
EndFunction
