State open
    Event OnTriggerEnter(ObjectReference akActionRef)
        Actor targetActor = akActionRef as Actor
        If targetActor != None && DeconArchSpell != None
            DeconArchSpell.Cast(Self, targetActor)
        EndIf
    EndEvent
EndState
