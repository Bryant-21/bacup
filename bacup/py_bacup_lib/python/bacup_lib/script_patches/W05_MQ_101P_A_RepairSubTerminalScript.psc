Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
    If auiMenuItemID == 1 && W05_MQ_101P_A != None && iPlayerSawScreen > 0
        If !W05_MQ_101P_A.IsStageDone(iPlayerSawScreen)
            W05_MQ_101P_A.SetStage(iPlayerSawScreen)
        EndIf
    EndIf
EndEvent
