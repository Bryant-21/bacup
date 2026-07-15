Function SetLinkedSwitchesOpen(ObjectReference akTerminalRef, Bool abOpen)
    ObjectReference[] linkedRefs = akTerminalRef.GetLinkedRefChain(LinkTerminalSwitchDoor, 100)
    Int i = 0
    While i < linkedRefs.Length
        Default2StateActivator linkedSwitch = linkedRefs[i] as Default2StateActivator
        If linkedSwitch != None
            linkedSwitch.IsAnimating = True
            linkedSwitch.SetOpenNoWait(abOpen)
        EndIf
        i = i + 1
    EndWhile
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    SetLinkedSwitchesOpen(akTerminalRef, True)
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    SetLinkedSwitchesOpen(akTerminalRef, True)
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    SetLinkedSwitchesOpen(akTerminalRef, False)
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    SetLinkedSwitchesOpen(akTerminalRef, False)
EndFunction
