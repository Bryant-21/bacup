; The dish base ENz01_OrientationDish carries DefaultMultiStateActivator states
; OFF/searching/ON and a BroadcastSoundRefScript. Orienting plays the search
; state and sync sound, then settles on ON after the retained animation length.
Event OnActivate(ObjectReference akActionRef)
    If akActionRef == None || akActionRef != Game.GetPlayer()
        Return
    EndIf
    Quest owner = GetOwningQuest()
    If owner == None || !owner.IsRunning() || owner.IsStopping()
        Return
    EndIf
    DefaultMultiStateActivator dish = GetReference() as DefaultMultiStateActivator
    If dish == None || dish.CurrentStateIndex != 0
        Return
    EndIf
    dish.SetLocalState(1)
    BroadcastSoundRefScript speaker = GetReference() as BroadcastSoundRefScript
    If speaker != None
        speaker.BroadcastSound()
    EndIf
    StartTimer(iAnimLengthTimer as Float, 1)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != 1
        Return
    EndIf
    DefaultMultiStateActivator dish = GetReference() as DefaultMultiStateActivator
    If dish != None && dish.CurrentStateIndex == 1
        dish.SetLocalState(2)
    EndIf
EndEvent

Event OnAliasShutdown()
    CancelTimer(1)
    DefaultMultiStateActivator dish = GetReference() as DefaultMultiStateActivator
    If dish != None && dish.CurrentStateIndex != 0
        dish.SetLocalState(0)
    EndIf
EndEvent
