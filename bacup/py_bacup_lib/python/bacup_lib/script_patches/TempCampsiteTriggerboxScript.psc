Event OnTriggerEnter(ObjectReference akActionRef)
    Actor enteringActor = akActionRef as Actor
    If enteringActor != None && TempInCampsite != None
        enteringActor.AddKeyword(TempInCampsite)
    EndIf
EndEvent

Event OnTriggerLeave(ObjectReference akActionRef)
    Actor leavingActor = akActionRef as Actor
    If leavingActor != None && TempInCampsite != None
        leavingActor.RemoveKeyword(TempInCampsite)
    EndIf
EndEvent
