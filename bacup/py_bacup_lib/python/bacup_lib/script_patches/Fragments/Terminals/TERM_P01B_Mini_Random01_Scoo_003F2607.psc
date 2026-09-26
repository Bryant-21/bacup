; Reading Sheepsquatch Field Research #4.
Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    Quest sinkingFeeling = Game.GetFormFromFile(0x0047F441, "SeventySix.esm") as Quest
    If sinkingFeeling && sinkingFeeling.IsRunning() && !sinkingFeeling.IsStageDone(425) && sinkingFeeling.IsStageDone(400)
        sinkingFeeling.SetStage(425)
    EndIf
EndFunction
