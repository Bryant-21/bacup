Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    If pBoSr01 != None && pBoSr01.IsRunning() && pBoSr01.IsStageDone(200) && !pBoSr01.IsStageDone(300)
        pBoSr01.SetStage(300)
    EndIf
EndFunction
