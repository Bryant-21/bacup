; The correct final passphrase digit unlocks Scoot's research.
Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    Quest sinkingFeeling = Game.GetFormFromFile(0x0047F441, "SeventySix.esm") as Quest
    If sinkingFeeling && sinkingFeeling.IsRunning() && !sinkingFeeling.IsStageDone(400)
        sinkingFeeling.SetStage(400)
    EndIf
EndFunction
