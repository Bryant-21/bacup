; FO4 TrapBase has no FO76 Trace wrapper. Keep the child Bool contract through
; the supported user-log API.

Bool Function Trace(ScriptObject CallingObject, String asTextToPrint, Int aiSeverity)
    Debug.OpenUserLog("Traps")
    Return Debug.TraceUser("Traps", CallingObject as String + ": " + asTextToPrint, aiSeverity)
EndFunction
