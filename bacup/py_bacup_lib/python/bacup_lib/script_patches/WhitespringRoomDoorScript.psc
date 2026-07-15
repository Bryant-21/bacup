Function UpdateLinkedScanner()
    WhitespringRoomHandScannerScript scanner = GetLinkedRef() as WhitespringRoomHandScannerScript
    If scanner != None
        scanner.SetNextState()
    EndIf
EndFunction

Event OnOpen(ObjectReference akActionRef)
    UpdateLinkedScanner()
EndEvent

Event OnClose(ObjectReference akActionRef)
    UpdateLinkedScanner()
EndEvent
