Function KillJohnnyInSecurityRoom()
    If Johnny == None
        Return
    EndIf

    Actor johnnyRef = Johnny.GetActorReference()
    If johnnyRef == None || johnnyRef.IsDead()
        Return
    EndIf
    StartTimer(JohnnyDeathTimerLength, JohnnyDeathTimerId)
EndFunction

Event OnTimer(int aiTimerID)
    If aiTimerID != JohnnyDeathTimerId || Johnny == None || Health == None
        Return
    EndIf

    Actor johnnyRef = Johnny.GetActorReference()
    If johnnyRef == None || johnnyRef.IsDead()
        Return
    EndIf
    johnnyRef.DamageValue(Health, HealthDamageAmount)
EndEvent
