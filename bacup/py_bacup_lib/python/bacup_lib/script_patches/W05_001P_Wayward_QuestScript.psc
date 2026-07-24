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
