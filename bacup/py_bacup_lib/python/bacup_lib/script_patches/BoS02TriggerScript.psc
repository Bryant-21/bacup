Event OnTriggerEnter(ObjectReference akActionRef)
    Actor enteringActor = akActionRef as Actor
    If enteringActor == None || pBoS02 == None
        Return
    EndIf
    If pBoS02CompletedAV != None && enteringActor.GetValue(pBoS02CompletedAV) >= 1.0
        Return
    EndIf
    If !pBoS02.IsRunning() && !pBoS02.IsCompleted()
        pBoS02.Start()
    EndIf
EndEvent
