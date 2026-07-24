Event OnInit()
    StartTimer(5.0, 1)
EndEvent

Event OnTimer(int aiTimerID)
    If aiTimerID != 1
        Return
    EndIf
    If DistanceCheckAlias == None || DistanceCheckAlias.GetReference() == None || RangeCheckDistance <= 0.0
        StartTimer(5.0, 1)
        Return
    EndIf
    Actor kPlayer = Game.GetPlayer()
    If kPlayer == None
        StartTimer(5.0, 1)
        Return
    EndIf
    If kPlayer.GetDistance(DistanceCheckAlias.GetReference()) <= RangeCheckDistance
        CancelTimer(1)
        SetStage(RangeCheckStage)
    Else
        StartTimer(5.0, 1)
    EndIf
EndEvent
