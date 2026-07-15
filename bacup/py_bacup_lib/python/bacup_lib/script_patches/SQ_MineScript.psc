; Parenthesize the linked-reference cast before invoking its method, and adapt
; FO76's four-argument TraceUser call to FO4's three-argument form.

Bool Function Trace(ScriptObject CallingObject, String asTextToPrint, Int aiSeverity) Global
    Debug.OpenUserLog("Mine")
    Return Debug.TraceUser("Mine", CallingObject as String + ": " + asTextToPrint, aiSeverity)
EndFunction

Function SpawnDefender()
    MineDefenderSpawner defenderSpawner = GetLinkedRef(LinkMineDefenderSpawner) as MineDefenderSpawner
    If defenderSpawner != None
        defenderSpawner.SpawnDefender(4, None)
    EndIf
EndFunction
