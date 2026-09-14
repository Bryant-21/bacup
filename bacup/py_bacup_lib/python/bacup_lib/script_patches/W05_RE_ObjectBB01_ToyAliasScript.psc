Event OnContainerChanged(ObjectReference akNewContainer, ObjectReference akOldContainer)
    If PlayerAlias == None
        Return
    EndIf

    ObjectReference PlayerRef = PlayerAlias.GetReference()
    Quest OwningQuest = GetOwningQuest()
    If PlayerRef != None && akNewContainer == PlayerRef && OwningQuest != None
        OwningQuest.SetStage(100)
    EndIf
EndEvent
