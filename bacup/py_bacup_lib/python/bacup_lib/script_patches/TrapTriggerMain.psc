; FO4-safe ownership fallback for the FO76 trap base. The original client PEX
; decompiles without a Bool return, which prevents this parent script from
; compiling and leaves WorkshopPressurePlateScript without a runtime parent.

Bool Function IsCurrentOwner(Actor akActor)
    If akActor == None
        Return False
    EndIf

    If akActor == Game.GetPlayer()
        Return True
    EndIf

    Return False
EndFunction

Bool Function Trace(ScriptObject CallingObject, String asTextToPrint, Int aiSeverity)
    Debug.Trace(asTextToPrint, aiSeverity)
    Return True
EndFunction
