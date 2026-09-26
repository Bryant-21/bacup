Function PlayWrapUpBroadcast(Topic akTopic, Actor akPlayer)
    If !IsRunning() || ExtSign == None || akTopic == None || akPlayer != Game.GetPlayer()
        Return
    EndIf
    ObjectReference signRef = ExtSign.GetReference()
    If signRef != None
        signRef.Say(akTopic, None, False, akPlayer)
    EndIf
EndFunction
