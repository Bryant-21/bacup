Function SetLinkedDeconArchesActive(ObjectReference akTerminalRef, Bool abActive)
    ObjectReference[] linkedRefs = akTerminalRef.GetLinkedRefChain(LinkTerminalDeconArch, 100)
    Int i = 0
    While i < linkedRefs.Length
        Default2StateActivator arch = linkedRefs[i] as Default2StateActivator
        If arch != None
            arch.IsAnimating = True
            arch.SetOpenNoWait(abActive)
        Else
            linkedRefs[i].Activate(akTerminalRef, False)
        EndIf
        i = i + 1
    EndWhile
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    SetLinkedDeconArchesActive(akTerminalRef, True)
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    SetLinkedDeconArchesActive(akTerminalRef, True)
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    SetLinkedDeconArchesActive(akTerminalRef, False)
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    SetLinkedDeconArchesActive(akTerminalRef, False)
EndFunction
