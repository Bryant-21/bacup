Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    ObjectReference[] linkedRefs = akTerminalRef.GetLinkedRefChain(LinkTerminalSwitchDoor, 100)
    Int i = 0
    While i < linkedRefs.Length
        Default2StateActivator linkedSwitch = linkedRefs[i] as Default2StateActivator
        If linkedSwitch != None
            linkedSwitch.IsAnimating = True
            linkedSwitch.SetOpenNoWait(True)
        EndIf
        i = i + 1
    EndWhile
EndFunction
