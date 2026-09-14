Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    ObjectReference linkedController = akTerminalRef.GetLinkedRef()
    If linkedController != None
        linkedController.Activate(akTerminalRef, False)
    EndIf
EndFunction
