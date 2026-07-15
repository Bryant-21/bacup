Event OnTriggerEnter(ObjectReference akActionRef)
    Actor enteringActor = akActionRef as Actor
    If enteringActor != None && KeywordToAdd != None
        enteringActor.AddKeyword(KeywordToAdd)
    EndIf
EndEvent

Event OnTriggerLeave(ObjectReference akActionRef)
    Actor leavingActor = akActionRef as Actor
    If leavingActor != None && KeywordToAdd != None
        leavingActor.RemoveKeyword(KeywordToAdd)
    EndIf
EndEvent
