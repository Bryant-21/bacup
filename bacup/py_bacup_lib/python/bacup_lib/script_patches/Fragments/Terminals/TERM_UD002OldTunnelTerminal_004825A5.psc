Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    Objects:UD002OldTunnelTerminal tunnel = akTerminalRef as Objects:UD002OldTunnelTerminal
    If tunnel == None || tunnel.LinkCustom03 == None
        Return
    EndIf
    ObjectReference tunnelDoor = akTerminalRef.GetLinkedRef(tunnel.LinkCustom03)
    If tunnelDoor
        tunnelDoor.Lock(False)
        tunnelDoor.SetOpen(True)
    EndIf
EndFunction
