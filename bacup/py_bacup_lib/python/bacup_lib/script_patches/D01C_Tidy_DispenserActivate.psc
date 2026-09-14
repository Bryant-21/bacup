Event OnActivate(ObjectReference akActionRef)
    If akActionRef != MyPlayer.GetReference()
        Return
    EndIf

    D01C_OperationTidyScript tidyQuest = GetOwningQuest() as D01C_OperationTidyScript
    If tidyQuest != None
        tidyQuest.CollectWaste(GetReference(), StageToSet)
    EndIf
EndEvent
