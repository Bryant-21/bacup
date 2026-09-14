Function Fragment_Terminal_05(ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && V94_HolotapeCommunityCouncil != None && playerRef.GetItemCount(V94_HolotapeCommunityCouncil) == 0
        playerRef.AddItem(V94_HolotapeCommunityCouncil, 1, False)
    EndIf
EndFunction
