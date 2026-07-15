Event OnTriggerEnter(ObjectReference akActionRef)
    If PlayerTriggerOnly && akActionRef != Game.GetPlayer()
        Return
    EndIf
    SendConfiguredStoryEvent()
EndEvent
