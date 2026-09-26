Function ReconcileTerminalState()
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || MoM02CTerminalValue == None
        Return
    EndIf
    If IsStageDone(80)
        playerRef.SetValue(MoM02CTerminalValue, CONST_MoM02CValue_ReadyForFabrication)
    ElseIf IsStageDone(70)
        playerRef.SetValue(MoM02CTerminalValue, CONST_MoM02CValue_ReadyForUpload)
    ElseIf IsStageDone(60)
        playerRef.SetValue(MoM02CTerminalValue, CONST_MoM02CValue_ReadyForDownload)
    ElseIf IsStageDone(52)
        playerRef.SetValue(MoM02CTerminalValue, CONST_MoM02CValue_ReadyForExfiltration)
    ElseIf IsStageDone(30)
        playerRef.SetValue(MoM02CTerminalValue, CONST_MoM02CValue_ReadyForTargets)
    EndIf
EndFunction
