; Reading Ghost Field Research #2.
Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    Quest sinkingFeeling = Game.GetFormFromFile(0x0047F441, "SeventySix.esm") as Quest
    If sinkingFeeling && sinkingFeeling.IsRunning() && !sinkingFeeling.IsStageDone(450) && sinkingFeeling.IsStageDone(400)
        sinkingFeeling.SetStage(450)
    EndIf
EndFunction
