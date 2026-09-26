Event OnQuestInit()
    Actor playerRef = Game.GetPlayer()
    If playerRef
        RegisterForRemoteEvent(playerRef, "OnLocationChange")
        CheckPlayerLocation(playerRef.GetCurrentLocation())
    EndIf
    ; FO76 set stage 103 ("Speak to Pennington") from Reclamation Day's end stage;
    ; the FO4 handoff only sends the start event, so the first objective never showed.
    ; Skip it when the player arrived via Roper or the Wayward triggers instead.
    Quest reclamationDay = Game.GetFormFromFile(0x000D4D34, "SeventySix.esm") as Quest
    If reclamationDay && (reclamationDay.IsStageDone(100) || reclamationDay.IsCompleted())
        If !IsStageDone(103) && !IsStageDone(105) && !IsStageDone(301) && !IsStageDone(400) && !IsStageDone(500)
            SetStage(103)
        EndIf
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
