; TODO

Event OnActivate(ObjectReference akActionRef)
    If Gail == None || akActionRef != Gail.GetReference()
        Return
    EndIf

    Quest owningQuest = GetOwningQuest()
    If owningQuest != None && owningQuest.GetStage() >= 600 && !owningQuest.IsStageDone(610)
        owningQuest.SetStage(610)
    EndIf
EndEvent
