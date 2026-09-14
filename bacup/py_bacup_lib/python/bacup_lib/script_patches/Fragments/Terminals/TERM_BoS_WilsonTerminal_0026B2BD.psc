Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    If pBoS02 != None && !pBoS02.IsStageDone(1900)
        pBoS02.SetStage(1900)
    EndIf
    If pBoS03 != None && !pBoS03.IsStageDone(200)
        pBoS03.SetStage(200)
    EndIf
EndFunction
