Event OnInit()
    RegisterForRemoteEvent(GetOwningQuest(), "OnStageSet")
EndEvent

Event Quest.OnStageSet(Quest akSender, int auiStageID, int auiItemID)
    If auiStageID != 1260
        Return
    EndIf
    If MoveToMarker != None && MoveToMarker.GetReference() != None
        MoveAllTo(MoveToMarker.GetReference())
    EndIf
EndEvent
