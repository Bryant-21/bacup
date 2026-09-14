Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && MoMHolotapeInfirmary != None && playerRef.GetItemCount(MoMHolotapeInfirmary) == 0
        playerRef.AddItem(MoMHolotapeInfirmary, 1, False)
    EndIf
    If akTerminalRef != None && MoMInfirmaryValue != None
        akTerminalRef.SetValue(MoMInfirmaryValue, 1.0)
    EndIf
EndFunction
