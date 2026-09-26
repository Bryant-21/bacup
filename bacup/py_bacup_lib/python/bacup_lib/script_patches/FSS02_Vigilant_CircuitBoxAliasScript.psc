Event OnActivate(ObjectReference akActionRef)
    Quest owner = GetOwningQuest()
    If owner == None || !owner.IsRunning() || akActionRef != Game.GetPlayer()
        Return
    EndIf

    ObjectReference circuitBox = GetReference()
    If circuitBox != None && circuitBox.IsDestroyed()
        circuitBox.Reset()
    EndIf

    ReferenceAlias terminalAlias = owner.GetAlias(9) as ReferenceAlias
    If terminalAlias == None
        Return
    EndIf
    ObjectReference terminalRef = terminalAlias.GetReference()
    If terminalRef != None && terminalRef.IsDestroyed()
        terminalRef.Reset()
    EndIf
EndEvent
