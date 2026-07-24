Event OnBegin(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
    If !SendOnEnd
        SendEvent(akSpeakerRef)
    EndIf
EndEvent

Event OnEnd(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
    If SendOnEnd
        SendEvent(akSpeakerRef)
    EndIf
EndEvent

Function SendEvent(ObjectReference akSpeakerRef)
    If EventKeyword == None
        Return
    EndIf

    Location eventLocation = akLoc
    If eventLocation == None && akSpeakerRef != None
        eventLocation = akSpeakerRef.GetCurrentLocation()
    EndIf

    ObjectReference eventRef1 = akRef1
    If eventRef1 == None
        eventRef1 = akSpeakerRef
    EndIf

    ObjectReference eventRef2 = akRef2
    If eventRef2 == None
        eventRef2 = Game.GetPlayer()
    EndIf

    EventKeyword.SendStoryEvent(eventLocation, eventRef1, eventRef2, aiValue1, aiValue2)
EndFunction
