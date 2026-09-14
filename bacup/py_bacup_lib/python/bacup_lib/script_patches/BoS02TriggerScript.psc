Event OnTriggerEnter(ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || akActionRef != playerRef || pBoS02 == None
        Return
    EndIf
    If pBoS02CompletedAV != None && playerRef.GetValue(pBoS02CompletedAV) >= 1.0
        Return
    EndIf
    If !pBoS02.IsRunning() && !pBoS02.IsCompleted()
        Keyword startKeyword = Game.GetFormFromFile(0x004E458E, "SeventySix.esm") as Keyword
        If startKeyword != None
            startKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        EndIf
    EndIf
EndEvent
