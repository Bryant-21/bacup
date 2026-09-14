Function SetProtectronStage(Int aiStage)
    If RE_SceneTS06 != None && !RE_SceneTS06.IsStageDone(aiStage)
        RE_SceneTS06.SetStage(aiStage)
    EndIf
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    SetProtectronStage(200)
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    SetProtectronStage(300)
EndFunction
