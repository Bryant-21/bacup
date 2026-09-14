Event OnQuestInit()
    Actor playerRef = MyPlayer.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        If WS01status != None
            playerRef.SetValue(WS01status, 0.0)
        EndIf
        RegisterForRemoteEvent(playerRef, "OnKill")
    EndIf
    If !IsStageDone(100)
        SetStage(100)
    EndIf
EndEvent

Event Actor.OnKill(Actor akSender, Actor akVictim)
    If akVictim == None || IsStageDone(StageToSet)
        Return
    EndIf
    If WhitespringFaction != None && !akVictim.IsInFaction(WhitespringFaction)
        Return
    EndIf
    If FeralGhoulGolfClothes != None && !akVictim.WornHasKeyword(FeralGhoulGolfClothes)
        Return
    EndIf
    Location victimLocation = akVictim.GetCurrentLocation()
    If LocationKeyword != None && (victimLocation == None || !victimLocation.HasKeyword(LocationKeyword))
        Return
    EndIf

    Actor playerRef = akSender
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef == None || WS01status == None || WS01MaxKills == None
        Return
    EndIf

    playerRef.SetValue(WS01status, playerRef.GetValue(WS01status) + 1.0)
    If playerRef.GetValue(WS01status) >= WS01MaxKills.GetValue()
        SetStage(StageToSet)
    EndIf
EndEvent

Event OnQuestShutdown()
    Actor playerRef = MyPlayer.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnKill")
    EndIf
EndEvent
