Event OnTriggerEnter(ObjectReference akActionRef)
    ObjectReference ghoulRef = GetLinkedRef()
    If akActionRef != ghoulRef
        Return
    EndIf

    Actor ghoulActor = ghoulRef as Actor
    If ghoulActor != None
        ghoulActor.Kill()
        Disable()
    EndIf
EndEvent
