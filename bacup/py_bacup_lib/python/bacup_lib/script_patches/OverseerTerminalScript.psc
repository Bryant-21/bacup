Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
    If SURV_Tutorial != None && SURV_Tutorial.IsRunning() && !SURV_Tutorial.IsStageDone(20)
        SURV_Tutorial.SetStage(20)
    EndIf
EndEvent
