Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    ObjectReference linkedDoor = akTerminalRef.GetLinkedRef()
    If linkedDoor != None
        linkedDoor.Unlock(False)
        linkedDoor.SetOpen(True)
    EndIf
EndFunction
