Event OnAliasInit()
    ObjectReference componentRef = GetRef()
    If componentRef != None
        componentRef.EnableNoWait()
    EndIf
EndEvent

Event OnContainerChanged(ObjectReference akNewContainer, ObjectReference akOldContainer)
    ObjectReference componentRef = GetRef()
    If componentRef == None || HoldingRef == None
        Return
    EndIf
    If akNewContainer == Game.GetPlayer()
        HoldingRef.AddRef(componentRef)
    ElseIf akOldContainer == Game.GetPlayer()
        HoldingRef.RemoveRef(componentRef)
    EndIf
EndEvent
