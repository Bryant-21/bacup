; Opening Wolf's emergency check-in is the 'read Wolf's terminal entry' step.
Function Fragment_Terminal_06(ObjectReference akTerminalRef)
    Quest loweDown = Game.GetFormFromFile(0x0047F443, "SeventySix.esm") as Quest
    If loweDown && loweDown.IsRunning() && !loweDown.IsStageDone(1150)
        loweDown.SetStage(1150)
    EndIf
EndFunction
