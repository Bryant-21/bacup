; Bureau of Tourism starts when the player first picks up WGRF Grafton Radio inside
; Toxic Valley. FO4 has no radio-reception event and the broadcast radius is not
; readable from Papyrus, so the bound Toxic Valley region stands in for it. The
; send matches the Overseer dialogue sender, and TW005 records status 2 on completion.
Event OnQuestInit()
    Actor player = Game.GetPlayer()
    RegisterForRemoteEvent(player, "OnLocationChange")
    SendTourismStartIfInValley(player)
EndEvent

Event Actor.OnLocationChange(Actor akSender, Location akOldLoc, Location akNewLoc)
    SendTourismStartIfInValley(akSender)
EndEvent

Function SendTourismStartIfInValley(Actor akPlayer)
    If akPlayer == None || akPlayer != Game.GetPlayer() || TW005_StartKeyword == None || RegionToxicValleyLocation == None || TW005status == None
        Return
    EndIf
    If akPlayer.GetValue(TW005status) >= StatusThreshhold || !akPlayer.IsInLocation(RegionToxicValleyLocation)
        Return
    EndIf
    TW005_StartKeyword.SendStoryEvent(None, akPlayer, akPlayer)
EndFunction
