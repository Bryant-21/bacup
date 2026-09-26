Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    Quest blackSheep = Game.GetFormFromFile(0x0046DC6F, "SeventySix.esm") as Quest
    If blackSheep && blackSheep.IsRunning() && !blackSheep.IsStageDone(350)
        If !blackSheep.IsStageDone(300)
            blackSheep.SetStage(300)
        EndIf
        blackSheep.SetStage(350)
    EndIf
EndFunction
