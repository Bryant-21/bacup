Event OnQuestInit()
    Actor playerRef = Game.GetPlayer()
    If playerRef
        RegisterForRemoteEvent(playerRef, "OnLocationChange")
        CheckPlayerLocation(playerRef.GetCurrentLocation())
    EndIf
EndEvent

Event Actor.OnLocationChange(Actor akSender, Location akOldLoc, Location akNewLoc)
    If akSender == Game.GetPlayer()
        CheckPlayerLocation(akNewLoc)
    EndIf
EndEvent

Function CheckPlayerLocation(Location playerLocation)
    If playerLocation == LocForestTheWaywardLocationInterior && !IsStageDone(102)
        SetStage(102)
    EndIf
EndFunction

Event OnStageSet(int auiStageID, int auiItemID)
    If auiStageID == 450 && Batter != None && Batter.GetActorReference() != None && !Batter.GetActorReference().IsDead()
        StartTimer(BatterFailsafeTimerLength, FailsafeID)
    EndIf
EndEvent

Event OnTimer(int aiTimerID)
    If aiTimerID == FailsafeID && Batter != None && Batter.GetActorReference() != None && !Batter.GetActorReference().IsDead()
        SetStage(KillBatterStage)
    EndIf
EndEvent
