Event OnActivate(ObjectReference akActivator)
    Actor playerRef = Game.GetPlayer()
    If akActivator != playerRef
        Return
    EndIf

    If EN02_MQ_Us != None && EN02_MQ_Us.IsCompleted()
        If LC080ElevatorDoorFoyer != None
            LC080ElevatorDoorFoyer.Activate(playerRef)
        EndIf
    ElseIf LC080ElevatorDoorExam != None
        LC080ElevatorDoorExam.Activate(playerRef)
    EndIf
EndEvent
