Bool Function Trace(ScriptObject CallingObject, String asTextToPrint, Int aiSeverity) Global
	String logName = "DangerRoom"
	debug.OpenUserLog(logName)
	; FO4 Debug.TraceUser takes (userLog, text, severity); the FO76 channel argument
	; has no equivalent and is dropped.
	Return debug.TraceUser(logName, CallingObject as String + ": " + asTextToPrint, aiSeverity)
EndFunction

Function SetStartingPositionReplacementText()
	; FO76 ObjectReference.AddTextReplacementValue(token, float) has no Fallout 4
	; equivalent -- FO4 only offers AddTextReplacementData(token, Form). BEHAVIOUR
	; LOST: the terminal ArrayStartingIndex numeric token is no longer replaced.
EndFunction
