; Tales owns the queen spawn, landing, fissure sealing and event clock, so the quest script only announces the kill.
Function AlphaKilled(Actor akAlpha)
    If !IsRunning() || akAlpha == None || myAlphaRef == akAlpha
        Return
    EndIf
    myAlphaRef = akAlpha
    If AlphaDeathMessage == None || IsStageDone(EventEndNoRewardStage)
        Return
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && NoAnnounceCells != None && NoAnnounceCells.Find(playerRef.GetParentCell()) >= 0
        Return
    EndIf
    AlphaDeathMessage.Show()
EndFunction
