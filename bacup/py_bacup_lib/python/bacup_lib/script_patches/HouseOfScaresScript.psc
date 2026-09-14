Event OnQuestInit()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnKill")
    EndIf
    If !IsStageDone(100)
        SetStage(100)
    EndIf
EndEvent

Event Actor.OnKill(Actor akSender, Actor akVictim)
    If akSender == None || akVictim == None || IsStageDone(StageToSet)
        Return
    EndIf
    If WendigoRace != None && akVictim.GetRace() == WendigoRace && HouseOfScaresOutfit != None && akSender.WornHasKeyword(HouseOfScaresOutfit)
        SetStage(StageToSet)
    EndIf
EndEvent

Event OnQuestShutdown()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnKill")
    EndIf
EndEvent
