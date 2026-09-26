Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    If Game.GetPlayer().GetItemCount(W05_MQ_102P_VTec_Holotape01) == 0
        Game.GetPlayer().AddItem(W05_MQ_102P_VTec_Holotape01, 1, False)
    EndIf
    ; Stage 550 ("Player gets the Security Recording") had no remaining setter.
    Quest overseen = Game.GetFormFromFile(0x003FFACF, "SeventySix.esm") as Quest
    If overseen && overseen.IsStageDone(530) && !overseen.IsStageDone(550) && !overseen.IsStageDone(560)
        overseen.SetStage(550)
    EndIf
EndFunction
