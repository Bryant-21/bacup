Event OnEnd(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
    Actor playerRef = Game.GetPlayer()
    Actor speakerRef = akSpeakerRef as Actor
    If ApplyFactionToSpeaker
        MarkActorForFaction(speakerRef)
    EndIf
    If ApplyFactionToTarget && (!ApplyFactionToSpeaker || speakerRef != playerRef)
        MarkActorForFaction(playerRef)
    EndIf
EndEvent

Function MarkActorForFaction(Actor akActor)
    If akActor == None || FactionToAdd == None
        Return
    EndIf
    akActor.AddToFaction(FactionToAdd)
    If TrackingValue != None
        akActor.SetValue(TrackingValue, NewTrackingValue)
    EndIf
EndFunction
