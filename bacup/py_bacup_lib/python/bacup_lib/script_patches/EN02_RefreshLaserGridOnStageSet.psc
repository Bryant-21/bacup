Event OnAliasInit()
    Quest owningQuest = GetOwningQuest()
    If owningQuest != None
        RegisterForRemoteEvent(owningQuest, "OnStageSet")
    EndIf
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    ObjectReference gridRef = GetRef()
    If gridRef != None && gridRef.Is3DLoaded()
        gridRef.Disable(False)
        gridRef.Enable(False)
    EndIf
EndEvent
