Function Fragment_Begin(ObjectReference akSpeakerRef)
    Actor player = Game.GetPlayer()
    If UnionPlayers != None && ConfederatePlayers != None && TW009_UnionSoldiers != None && TW009_ConfederateSoldiers != None
        ConfederatePlayers.RemoveRef(player)
        player.RemoveFromFaction(TW009_ConfederateSoldiers)
        UnionPlayers.AddRef(player)
        player.AddToFaction(TW009_UnionSoldiers)
        If TW009UnionJoinMsg != None
            TW009UnionJoinMsg.Show()
        EndIf
    EndIf
EndFunction
