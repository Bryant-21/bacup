Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    ObjectReference[] switchRefs = akTerminalRef.GetLinkedRefChain(LinkTerminalSwitchDoor, 100)
    Int i = 0
    While i < switchRefs.Length
        Default2StateActivator linkedSwitch = switchRefs[i] as Default2StateActivator
        If linkedSwitch != None
            linkedSwitch.IsAnimating = True
            linkedSwitch.SetOpenNoWait(True)
        EndIf
        i = i + 1
    EndWhile

    ObjectReference[] doorRefs = akTerminalRef.GetLinkedRefChain(LinkTerminalDoor, 100)
    i = 0
    While i < doorRefs.Length
        doorRefs[i].Unlock(False)
        doorRefs[i].SetOpen(True)
        i = i + 1
    EndWhile
EndFunction
