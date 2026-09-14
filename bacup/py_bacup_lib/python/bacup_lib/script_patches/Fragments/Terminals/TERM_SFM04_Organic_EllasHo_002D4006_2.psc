Function Fragment_Terminal_05(ObjectReference akTerminalRef)
    If SFM04_Organic != None && !SFM04_Organic.IsStageDone(300)
        SFM04_Organic.SetStage(300)
    EndIf
EndFunction
