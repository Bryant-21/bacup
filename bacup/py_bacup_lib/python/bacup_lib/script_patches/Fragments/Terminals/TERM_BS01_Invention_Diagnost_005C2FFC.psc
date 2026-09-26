Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    Quest invention = Game.GetFormFromFile(0x005B79EB, "SeventySix.esm") as Quest
    If invention != None && invention.IsRunning() && invention.IsStageDone(700) && !invention.IsStageDone(710)
        invention.SetStage(710)
    EndIf
EndFunction
