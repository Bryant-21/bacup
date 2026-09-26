; Downloading the coordinates from Wolf's emergency check-in also counts as reading it.
Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    Quest loweDown = Game.GetFormFromFile(0x0047F443, "SeventySix.esm") as Quest
    If loweDown && loweDown.IsRunning() && !loweDown.IsStageDone(1150)
        loweDown.SetStage(1150)
    EndIf
EndFunction
