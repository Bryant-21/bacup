Event OnAliasInit()
    Int i = 0
    While i < GetCount()
        ObjectReference terminalRef = GetAt(i)
        If terminalRef != None
            terminalRef.BlockActivation(False, False)
        EndIf
        i += 1
    EndWhile
EndEvent
