Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    If pBoS03 != None && !pBoS03.IsStageDone(600)
        pBoS03.SetStage(600)
    EndIf
EndFunction
