Function ActivateStealth()
    Actor jenRef = GetActorReference()
    If jenRef == None || W05_MQS_205P_JenStealthSpell == None
        Return
    EndIf

    CancelTimer(1)
    W05_MQS_205P_JenStealthSpell.Cast(jenRef, jenRef)
    StartTimer(SpellDuration, 1)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1 && W05_MQS_205P_JenStealthFieldOff != None && !W05_MQS_205P_JenStealthFieldOff.IsPlaying()
        W05_MQS_205P_JenStealthFieldOff.Start()
    EndIf
EndEvent
