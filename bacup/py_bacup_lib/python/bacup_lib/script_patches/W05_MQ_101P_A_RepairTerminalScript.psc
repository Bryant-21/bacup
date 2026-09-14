Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || W05_MQ_101P_A == None || W05_MQ_101P_A_RepairTerminalKey == None
        Return
    EndIf
    If akActionRef.GetItemCount(W05_MQ_101P_A_RepairTerminalKey) < 1 && !W05_MQ_101P_A.IsStageDone(iStageGetPasscode)
        W05_MQ_101P_A.SetStage(iStageGetPasscode)
    EndIf
EndEvent
