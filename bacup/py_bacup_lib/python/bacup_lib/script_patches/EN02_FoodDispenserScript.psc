Event OnActivate(ObjectReference akActionRef)
    Actor playerRef = akActionRef as Actor
    If playerRef == None || playerRef != Game.GetPlayer() || RequiredActorValue == None
        Return
    EndIf
    If playerRef.GetValue(RequiredActorValue) >= RequiredAVValue
        If ItemToGive != None
            playerRef.AddItem(ItemToGive, 1, False)
        EndIf
        playerRef.SetValue(RequiredActorValue, AVValueToSet)
    EndIf
EndEvent
