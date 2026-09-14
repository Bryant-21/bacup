Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && V94_HolotapeGECK != None && playerRef.GetItemCount(V94_HolotapeGECK) == 0
        playerRef.AddItem(V94_HolotapeGECK, 1, False)
    EndIf
EndFunction
