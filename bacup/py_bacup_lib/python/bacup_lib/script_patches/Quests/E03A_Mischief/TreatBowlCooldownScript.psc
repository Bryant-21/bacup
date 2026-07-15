Event OnActivate(ObjectReference akActionRef)
    Actor activatingPlayer = akActionRef as Actor
    If activatingPlayer != Game.GetPlayer()
        Return
    EndIf

    If activatingPlayer.GetValue(CooldownAV) > 0.0
        Return
    EndIf

    activatingPlayer.SetValue(CooldownAV, 1.0)
    BlockActivation(True, False)
    StartTimer(CooldownSeconds, 0)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 0
        Game.GetPlayer().SetValue(CooldownAV, 0.0)
        BlockActivation(False, False)
    EndIf
EndEvent
