Event OnTriggerEnter(ObjectReference akActionRef)
    Actor enteringActor = akActionRef as Actor
    If enteringActor != Game.GetPlayer()
        Return
    EndIf

    If enteringActor.GetValue(W05_MQ_003P_Muscle_PlayerCanAccessDuncan) > 0.0
        ObjectReference hiddenDoor = GetLinkedRef(None)
        If hiddenDoor != None
            hiddenDoor.SetOpen(True)
        EndIf
    EndIf
EndEvent
