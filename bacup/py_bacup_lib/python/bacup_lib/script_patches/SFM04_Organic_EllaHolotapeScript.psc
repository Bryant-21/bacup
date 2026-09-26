Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
    If SFM04_Organic == None || !SFM04_Organic.IsRunning() || SFM04_Organic.IsStageDone(1000)
        Return
    EndIf
    If auiMenuItemID == 4 || auiMenuItemID == 5
        If !SFM04_Organic.IsStageDone(300)
            SFM04_Organic.SetStage(300)
        EndIf
        If auiMenuItemID == 5 && !SFM04_Organic.IsStageDone(315) && !SFM04_Organic.IsStageDone(310)
            SFM04_Organic.SetStage(310)
        EndIf
    EndIf
EndEvent
