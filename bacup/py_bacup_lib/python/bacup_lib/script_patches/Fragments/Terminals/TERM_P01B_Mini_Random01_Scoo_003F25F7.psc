; Opening Scoot's private research entry is the 'attempted private entry' step.
Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    Quest sinkingFeeling = Game.GetFormFromFile(0x0047F441, "SeventySix.esm") as Quest
    If sinkingFeeling && sinkingFeeling.IsRunning() && !sinkingFeeling.IsStageDone(300)
        sinkingFeeling.SetStage(300)
    EndIf
EndFunction
