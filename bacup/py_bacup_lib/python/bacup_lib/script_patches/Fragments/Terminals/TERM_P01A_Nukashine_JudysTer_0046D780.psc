Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef && P01A_Nukashine_DistillerySupplyPassword && playerRef.GetItemCount(P01A_Nukashine_DistillerySupplyPassword) == 0
        playerRef.AddItem(P01A_Nukashine_DistillerySupplyPassword, 1, False)
    EndIf
EndFunction
