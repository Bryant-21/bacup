; Viewing the reply to Wolf's last check-in is the quest's "read last check-in" step.
Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    Quest blackSheep = Game.GetFormFromFile(0x0046DC6F, "SeventySix.esm") as Quest
    If blackSheep && blackSheep.IsRunning() && !blackSheep.IsStageDone(300)
        blackSheep.SetStage(300)
    EndIf
EndFunction
