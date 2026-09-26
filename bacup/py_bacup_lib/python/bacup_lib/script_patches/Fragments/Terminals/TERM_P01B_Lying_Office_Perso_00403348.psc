; Reading Calvin's reminder to meet Bo-Peep ends Lying Lowe (900 -> 950 -> 9000).
Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    Quest lyingLowe = Game.GetFormFromFile(0x00478DD3, "SeventySix.esm") as Quest
    If lyingLowe && lyingLowe.IsRunning() && !lyingLowe.IsStageDone(900)
        If !lyingLowe.IsStageDone(800)
            lyingLowe.SetStage(800)
        EndIf
        lyingLowe.SetStage(900)
    EndIf
EndFunction
