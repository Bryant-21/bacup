Function BroadcastDuchessShout()
    If Duchess == None || Duchess.GetActorReference() == None
        Return
    EndIf
    Actor duchessActor = Duchess.GetActorReference()

    Int i = 0
    Int count = (Self as RefCollectionAlias).GetCount()
    While i < count
        ObjectReference patron = (Self as RefCollectionAlias).GetAt(i)
        If patron != None
            duchessActor.Say(W05_Wayward_DuchessViolenceShouts, None, False, patron)
        EndIf
        i += 1
    EndWhile
EndFunction
