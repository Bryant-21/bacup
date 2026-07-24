Event OnDeath(Actor akKiller)
    StartTimer(fDeathTimerDuration, 1)
EndEvent

Event OnTimer(int aiTimerID)
    If aiTimerID == 1 && DeathExplosion != None
        PlaceAtMe(DeathExplosion)
    EndIf
EndEvent
