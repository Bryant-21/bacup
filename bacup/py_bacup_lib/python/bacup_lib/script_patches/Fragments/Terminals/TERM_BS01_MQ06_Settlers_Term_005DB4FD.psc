Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    Quest settlers = Game.GetFormFromFile(0x005D1F89, "SeventySix.esm") as Quest
    If settlers != None && settlers.IsRunning() && settlers.IsStageDone(350) && !settlers.IsStageDone(425) && !settlers.IsStageDone(450)
        settlers.SetStage(425)
    EndIf
EndFunction
