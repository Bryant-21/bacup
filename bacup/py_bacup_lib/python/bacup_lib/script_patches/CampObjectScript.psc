; FO76 Debug.TraceLog returns Bool, but FO4's compatibility declaration does not.
; Preserve the caller contract with FO4's user-log API.

Bool Function Trace(ScriptObject CallingObject, String asTextToPrint, Int aiSeverity, String DejaSubChannel, Bool bShowNormalTrace) Global
	Debug.OpenUserLog("Camp")
	Return Debug.TraceUser("Camp", CallingObject as String + ": " + asTextToPrint, aiSeverity)
EndFunction
