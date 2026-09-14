Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    ObjectReference linkedDoor = akTerminalRef.GetLinkedRef()
    If linkedDoor != None
        linkedDoor.Unlock(False)
        linkedDoor.SetOpen(True)
    EndIf
EndFunction
