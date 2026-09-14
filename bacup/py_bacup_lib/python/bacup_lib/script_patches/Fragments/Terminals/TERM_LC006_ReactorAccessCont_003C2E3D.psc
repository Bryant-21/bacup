Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    ObjectReference linkedDoor = akTerminalRef.GetLinkedRef()
    If linkedDoor != None
        linkedDoor.Unlock(False)
    EndIf
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    ObjectReference linkedDoor = akTerminalRef.GetLinkedRef()
    If linkedDoor != None
        linkedDoor.SetOpen(True)
    EndIf
EndFunction
