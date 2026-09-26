Function UploadData(ObjectReference akTerminalRef, Bool dummyData)
    Quest raiders = Game.GetFormFromFile(0x005D2AFF, "SeventySix.esm") as Quest
    If raiders == None || !raiders.IsRunning() || !raiders.IsStageDone(1000)
        Return
    EndIf
    ReferenceAlias valdezTerminal = raiders.GetAlias(16) as ReferenceAlias
    If valdezTerminal == None || akTerminalRef == None || akTerminalRef != valdezTerminal.GetReference()
        Return
    EndIf
    If raiders.IsStageDone(1100) || raiders.IsStageDone(1150) || raiders.IsStageDone(1190)
        Return
    EndIf
    If dummyData
        If raiders.IsStageDone(1050)
            raiders.SetStage(1150)
        EndIf
    Else
        raiders.SetStage(1100)
    EndIf
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    UploadData(akTerminalRef, False)
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    UploadData(akTerminalRef, True)
EndFunction
