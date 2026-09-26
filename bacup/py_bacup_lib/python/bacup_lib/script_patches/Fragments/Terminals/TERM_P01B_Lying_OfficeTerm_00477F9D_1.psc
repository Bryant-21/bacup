; Calvin's office terminal: the locked Research entry (item 1) and the entry unlocked by
; the security code (item 3) drive Lying Lowe's 550 and 800 steps.
Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    P01B_Lying01_TrySetStage(550)
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    P01B_Lying01_TrySetStage(800)
EndFunction

Function P01B_Lying01_TrySetStage(Int aiStage)
    Quest lyingLowe = Game.GetFormFromFile(0x00478DD3, "SeventySix.esm") as Quest
    If lyingLowe && lyingLowe.IsRunning() && !lyingLowe.IsStageDone(aiStage)
        lyingLowe.SetStage(aiStage)
    EndIf
EndFunction
