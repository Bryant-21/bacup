Event OnTriggerEnter(ObjectReference akActionRef)
    Actor targetActor = akActionRef as Actor
    If targetActor == Game.GetPlayer() && GetState() == "open"
        DeconArchSpell.Cast(Self, targetActor)
    EndIf
EndEvent
