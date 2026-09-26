Event OnEnd(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
    If MapMarker == None
        Return
    EndIf
    If ListenerRange > 0
        Actor playerRef = Game.GetPlayer()
        If akSpeakerRef == None || playerRef == None || akSpeakerRef.GetDistance(playerRef) > ListenerRange
            Return
        EndIf
    EndIf
    MapMarker.AddToMap()
EndEvent
