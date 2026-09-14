Function Fragment_Begin(ObjectReference akSpeakerRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && PlayerGives != None && PlayerGetsCoin != None
        playerRef.RemoveItem(PlayerGives, 1, True, akSpeakerRef)
        playerRef.AddItem(PlayerGetsCoin, 1, False)
    EndIf
EndFunction
