Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    If MessageToShow.Show() != 0
        Return
    EndIf

    W05_MQ_003P_Muscle.SetStage(StageToSet)
    ObjectReference linkedTracker = GetLinkedRef(W05_MQ_003P_Muscle_LinkedTracker)
    If linkedTracker != None
        linkedTracker.Disable(False)
    EndIf
    BlockActivation(True, True)
EndEvent
