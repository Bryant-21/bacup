Event OnActivate(ObjectReference akActionRef)
    If !IsPowered() || numOfResults <= 0
        Return
    EndIf

    GoToState("busy")
    ResultAnimationIdx = Utility.RandomInt(0, numOfResults - 1)
    PlayAnimationOnClients()
    GoToState("Waiting")
EndEvent
