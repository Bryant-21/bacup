Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && MoMHolotapeCouncil != None && playerRef.GetItemCount(MoMHolotapeCouncil) == 0
        playerRef.AddItem(MoMHolotapeCouncil, 1, False)
    EndIf
    If akTerminalRef != None && MoMCouncilChamberTerminalValue != None
        akTerminalRef.SetValue(MoMCouncilChamberTerminalValue, 1.0)
    EndIf
EndFunction
