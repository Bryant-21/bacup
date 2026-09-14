Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef && Holotape_End && playerRef.GetItemCount(Holotape_End) == 0
        playerRef.AddItem(Holotape_End, 1, False)
        If AV_gotHolotape
            playerRef.SetValue(AV_gotHolotape, 1.0)
        EndIf
    EndIf
EndFunction
