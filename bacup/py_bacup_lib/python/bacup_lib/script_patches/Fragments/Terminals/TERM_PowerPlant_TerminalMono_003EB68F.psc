; Menu item 1 is the master control terminal's successful "Restart Power Plant" response, so the
; fragment hands the restart to PowerPlantTerminalScript, which only accepts it while the terminal
; reports ReadyForRestart. The linked-ref activation is the terminal's local controller action.
Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    If akTerminalRef == None
        Return
    EndIf
    ObjectReference linkedController = akTerminalRef.GetLinkedRef()
    If linkedController != None
        linkedController.Activate(akTerminalRef, False)
    EndIf
    PowerPlantTerminalScript powerPlantTerminal = akTerminalRef.GetBaseObject() as PowerPlantTerminalScript
    If powerPlantTerminal != None
        powerPlantTerminal.RequestPowerPlantRestart(akTerminalRef)
    EndIf
EndFunction
