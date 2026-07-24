Event OnActivate(ObjectReference akActionRef)
    GoToState("startstate")
    Int i = 0
    While i < numExplosions
        Utility.Wait(Utility.RandomFloat(minTimeBetweenExplosions, maxTimeBetweenExposions))
        PlaceAtMe(myExplosion)
        i += 1
    EndWhile
    GoToState("donestate")
EndEvent

State startstate
    Event OnActivate(ObjectReference akActionRef)
    EndEvent
EndState

State donestate
    Event OnActivate(ObjectReference akActionRef)
    EndEvent
EndState
