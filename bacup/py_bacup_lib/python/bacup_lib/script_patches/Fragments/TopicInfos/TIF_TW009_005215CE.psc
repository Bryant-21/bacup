Function Fragment_Begin(ObjectReference akSpeakerRef)
    Actor player = Game.GetPlayer()
    If UnionPlayers != None && ConfederatePlayers != None && TW009_UnionSoldiers != None && TW009_ConfederateSoldiers != None
        UnionPlayers.RemoveRef(player)
        player.RemoveFromFaction(TW009_UnionSoldiers)
        ConfederatePlayers.AddRef(player)
        player.AddToFaction(TW009_ConfederateSoldiers)
        If TW009ConfederateJoinMsg != None
            TW009ConfederateJoinMsg.Show()
        EndIf
    EndIf
EndFunction
