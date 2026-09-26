Function ExtractCPU(Int resultStage)
    Quest invention = Game.GetFormFromFile(0x005B79EB, "SeventySix.esm") as Quest
    If invention == None || !invention.IsRunning() || !invention.IsStageDone(900)
        Return
    EndIf
    If invention.IsStageDone(910) || invention.IsStageDone(911) || invention.IsStageDone(912) || invention.IsStageDone(913)
        Return
    EndIf
    invention.SetStage(resultStage)
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    ExtractCPU(911)
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    ExtractCPU(912)
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    ExtractCPU(913)
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    ExtractCPU(910)
EndFunction
