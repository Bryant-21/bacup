Event OnActivate(ObjectReference akActionRef)
    Actor activatingPlayer = akActionRef as Actor

    If activatingPlayer == None || activatingPlayer != Game.GetPlayer()
        Return
    EndIf

    If !IsPowered() || DeconArchSpell == None
        Return
    EndIf

    String currentState = GetState()
    If currentState == "startsopen" || currentState == "open"
        Return
    EndIf

    GoToState("startsopen")
    SetOpen(True)
    GoToState("open")
    DeconArchSpell.Cast(Self, activatingPlayer)
    StartTimer(3.0, CONST_DeconArchTimerID)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == CONST_DeconArchTimerID
        GoToState("startsclosed")
        SetOpen(False)
        GoToState("closed")
    Else
        Parent.OnTimer(aiTimerID)
    EndIf
EndEvent
