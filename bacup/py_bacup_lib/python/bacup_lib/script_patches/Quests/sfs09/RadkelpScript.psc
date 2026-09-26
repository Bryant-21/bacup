; A picked radkelp leaves the in-level collection; the quest's respawn tick regrows its marker.
Event OnContainerChanged(ObjectReference akSenderRef, ObjectReference akNewContainer, ObjectReference akOldContainer)
    If akSenderRef != None && akNewContainer != None
        RemoveRef(akSenderRef)
    EndIf
EndEvent
