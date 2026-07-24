Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        SetLinkedDoorsOpen(True)
    EndIf
EndEvent

Event OnTriggerLeave(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        SetLinkedDoorsOpen(False)
    EndIf
EndEvent

Function SetLinkedDoorsOpen(Bool shouldOpen)
    ObjectReference firstDoor = GetLinkedRef(LinkCustom01)
    ObjectReference secondDoor = GetLinkedRef(LinkCustom02)

    If firstDoor != None
        firstDoor.SetOpen(shouldOpen)
    EndIf
    If secondDoor != None
        secondDoor.SetOpen(shouldOpen)
    EndIf
EndFunction

; TODO
