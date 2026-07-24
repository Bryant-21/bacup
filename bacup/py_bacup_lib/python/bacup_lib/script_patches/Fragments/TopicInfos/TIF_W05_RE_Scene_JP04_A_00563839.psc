Function Fragment_End(ObjectReference akSpeakerRef)
    If PlayerRef != None
        PlayerRef.ForceRefTo(Game.GetPlayer())
    EndIf
    Actor targetActor = None
    If PlayerRef != None
        targetActor = PlayerRef.GetActorReference()
    EndIf
    If Raider1 != None && targetActor != None
        Actor raider1Actor = Raider1.GetActorReference()
        If raider1Actor != None
            raider1Actor.StartCombat(targetActor)
        EndIf
    EndIf
    If Raider2 != None && targetActor != None
        Actor raider2Actor = Raider2.GetActorReference()
        If raider2Actor != None
            raider2Actor.StartCombat(targetActor)
        EndIf
    EndIf
EndFunction
