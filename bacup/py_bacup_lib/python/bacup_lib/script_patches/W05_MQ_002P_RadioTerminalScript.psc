Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
    If auiMenuItemID == TargetMenuItem && W05_MQ_002P_Radical != None && StageToSet > 0
        If !W05_MQ_002P_Radical.IsStageDone(StageToSet)
            W05_MQ_002P_Radical.SetStage(StageToSet)
        EndIf
    EndIf
EndEvent
