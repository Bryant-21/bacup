Function RegisterForCloseTrigger()
    ObjectReference triggerRef = GetLinkedRef(LinkTrigger)
    If triggerRef != None
        RegisterForRemoteEvent(triggerRef, "OnTriggerEnter")
    EndIf
EndFunction

Function SetOpen(Bool abOpen = True)
    CancelTimer(451)
    Parent.SetOpen(abOpen)
    If abOpen
        StartTimer(15.0, 451)
    EndIf
EndFunction

Event OnLoad()
    Parent.OnLoad()
    RegisterForCloseTrigger()
EndEvent

Event OnReset()
    Parent.OnReset()
    RegisterForCloseTrigger()
EndEvent

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
    If akSender == GetLinkedRef(LinkTrigger) && akActionRef == Game.GetPlayer() && IsOpen
        SetOpen(False)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 451
        If IsOpen
            SetOpen(False)
        EndIf
    Else
        Parent.OnTimer(aiTimerID)
    EndIf
EndEvent
