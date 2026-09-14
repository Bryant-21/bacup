Function Fragment_End(ObjectReference akSpeakerRef)
    If akSpeakerRef != None && SelfDestructExplosion != None
        akSpeakerRef.PlaceAtMe(SelfDestructExplosion)
        Actor speakerActor = akSpeakerRef as Actor
        If speakerActor != None
            speakerActor.Kill()
        EndIf
    EndIf
EndFunction
