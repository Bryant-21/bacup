Bool Function Trace(ScriptObject CallingObject, String asTextToPrint, Int aiSeverity, String DejaSubChannel, Bool bShowNormalTrace)
    Debug.OpenUserLog("FlipCardSign")
    Return Debug.TraceUser("FlipCardSign", CallingObject as String + ": " + asTextToPrint, aiSeverity)
EndFunction
