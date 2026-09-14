Event OnWorkshopObjectPlaced(ObjectReference akReference)
    If W05_MQ_002P_Radical && !W05_MQ_002P_Radical.IsStageDone(CompletedSectionStage)
        If !W05_MQ_002P_Radical.IsStageDone(270)
            W05_MQ_002P_Radical.SetStage(270)
        EndIf
        If !W05_MQ_002P_Radical.IsStageDone(400)
            W05_MQ_002P_Radical.SetStage(400)
        EndIf
    EndIf
EndEvent

Event OnPowerOn(ObjectReference akPowerGenerator)
    If W05_MQ_002P_Radical && !W05_MQ_002P_Radical.IsStageDone(400)
        W05_MQ_002P_Radical.SetStage(400)
    EndIf
EndEvent

Event OnWorkshopObjectDestroyed(ObjectReference akActionRef)
    If W05_MQ_002P_Radical && W05_MQ_002P_Radical.IsStageDone(400) && !W05_MQ_002P_Radical.IsStageDone(450)
        W05_MQ_002P_Radical.SetObjectiveDisplayed(375, True, True)
    EndIf
EndEvent
