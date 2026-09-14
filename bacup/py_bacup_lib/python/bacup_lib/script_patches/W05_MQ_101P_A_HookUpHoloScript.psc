Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
    If auiMenuItemID == 1 && W05_MQ_101P_A && W05_MQ_101P_A.IsStageDone(950) && !W05_MQ_101P_A.IsStageDone(iStagePlayBroadcast)
        W05_MQ_101P_A.SetStage(iStagePlayBroadcast)
    EndIf
EndEvent
