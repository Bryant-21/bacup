; Retrieving Session 4's log ejects Calvin's recording (stage 1100 hands the holotape over).
Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    Quest loweDown = Game.GetFormFromFile(0x0047F443, "SeventySix.esm") as Quest
    If loweDown && loweDown.IsRunning() && !loweDown.IsStageDone(1100)
        loweDown.SetStage(1100)
    EndIf
EndFunction
