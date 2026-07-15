; FO76 Debug.TraceLog is unavailable in FO4. Preserve the Bool contract with
; FO4's user-log API.

Bool Function Trace(ScriptObject CallingObject, String asTextToPrint, Int aiSeverity, String DejaSubChannel, Bool bShowNormalTrace)
    Debug.OpenUserLog("Nukes")
    Return Debug.TraceUser("Nukes", CallingObject as String + ": " + asTextToPrint, aiSeverity)
EndFunction
