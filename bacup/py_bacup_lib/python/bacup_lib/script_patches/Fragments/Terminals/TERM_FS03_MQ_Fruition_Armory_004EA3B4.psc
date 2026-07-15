Function SetLinkedDoorsOpen(ObjectReference akTerminalRef, Bool abOpen)
    ObjectReference[] linkedRefs = akTerminalRef.GetLinkedRefChain(LinkTerminalDoor, 100)
    Int i = 0
    While i < linkedRefs.Length
        linkedRefs[i].SetOpen(abOpen)
        i = i + 1
    EndWhile
EndFunction

Function UnlockLinkedDoors(ObjectReference akTerminalRef)
    ObjectReference[] linkedRefs = akTerminalRef.GetLinkedRefChain(LinkTerminalDoor, 100)
    Int i = 0
    While i < linkedRefs.Length
        linkedRefs[i].Unlock(False)
        i = i + 1
    EndWhile
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    SetLinkedDoorsOpen(akTerminalRef, True)
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    SetLinkedDoorsOpen(akTerminalRef, True)
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    UnlockLinkedDoors(akTerminalRef)
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    UnlockLinkedDoors(akTerminalRef)
EndFunction
