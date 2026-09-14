Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef && PatrolHolotape && playerRef.GetItemCount(PatrolHolotape) == 0
        playerRef.AddItem(PatrolHolotape, 1, False)
        If GotHolotape
            playerRef.SetValue(GotHolotape, 1.0)
        EndIf
        If pRSVP00_AV_Terminal_or_Camp
            playerRef.SetValue(pRSVP00_AV_Terminal_or_Camp, 1.0)
        EndIf
    EndIf
EndFunction
