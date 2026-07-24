Function Fragment_End(ObjectReference akSpeakerRef)
    If PlayerRef != None
        PlayerRef.ForceRefTo(Game.GetPlayer())
    EndIf
    If Raider2 != None && PlayerRef != None
        Actor raider2Actor = Raider2.GetActorReference()
        Actor targetActor = PlayerRef.GetActorReference()
        If raider2Actor != None && targetActor != None
            raider2Actor.StartCombat(targetActor)
        EndIf
    EndIf
EndFunction
