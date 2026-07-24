Function Fragment_End(ObjectReference akSpeakerRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && pRS01A_CheckpointValue != None
        playerRef.SetValue(pRS01A_CheckpointValue, 1.0)
    EndIf
    If playerRef != None && pRS01A_Contact_Keyword != None
        pRS01A_Contact_Keyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction
