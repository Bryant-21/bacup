Function Fragment_End(ObjectReference akSpeakerRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && pRS03_Inoculation_Keyword != None
        pRS03_Inoculation_Keyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction
