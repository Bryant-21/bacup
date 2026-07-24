Event OnActivate(ObjectReference akActionRef)
    Actor triggeringActor = akActionRef as Actor
    If !triggeringActor
        Return
    EndIf

    If triggeringActor.GetValue(CooldownAV) > Utility.GetCurrentRealTime()
        Return
    EndIf

    triggeringActor.SetValue(CooldownAV, Utility.GetCurrentRealTime() + CooldownSeconds)
EndEvent
