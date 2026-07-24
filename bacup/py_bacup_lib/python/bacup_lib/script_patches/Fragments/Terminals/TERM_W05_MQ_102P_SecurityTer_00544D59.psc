Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    If Game.GetPlayer().GetItemCount(W05_MQ_102P_VTec_Holotape01) == 0
        Game.GetPlayer().AddItem(W05_MQ_102P_VTec_Holotape01, 1, False)
    EndIf
EndFunction
