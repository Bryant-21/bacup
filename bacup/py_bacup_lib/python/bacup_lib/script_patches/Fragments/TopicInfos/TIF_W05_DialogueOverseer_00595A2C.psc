Function Fragment_End(ObjectReference akSpeakerRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && pTW005_StartKeyword != None
        pTW005_StartKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction
