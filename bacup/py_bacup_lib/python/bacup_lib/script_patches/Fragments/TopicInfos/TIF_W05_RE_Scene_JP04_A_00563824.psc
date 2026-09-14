Function Fragment_End(ObjectReference akSpeakerRef)
    If Raider2 != None && Game.GetPlayer() != None
        Actor raiderActor = Raider2.GetActorReference()
        If raiderActor != None
            raiderActor.StartCombat(Game.GetPlayer())
        EndIf
    EndIf
EndFunction
