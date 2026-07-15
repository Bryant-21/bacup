Event OnAliasInit()
    Quest owningQuest = GetOwningQuest()
    If owningQuest != None
        RegisterForRemoteEvent(owningQuest, "OnStageSet")
    EndIf
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    If auiStageID == iTriggeringStage
        RefreshLaserGrids()
    EndIf
EndEvent

Function RefreshLaserGrids()
    Int i = 0
    While i < GetCount()
        ObjectReference gridRef = GetAt(i)
        If gridRef != None && gridRef.Is3DLoaded()
            gridRef.Disable(False)
            gridRef.Enable(False)
        EndIf
        i = i + 1
    EndWhile
EndFunction
