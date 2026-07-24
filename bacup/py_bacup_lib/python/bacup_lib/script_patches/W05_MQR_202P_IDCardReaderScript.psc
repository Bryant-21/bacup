Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    Quest owningQuest = GetOwningQuest()
    If owningQuest != None && !owningQuest.IsStageDone(550)
        owningQuest.SetStage(550)
    EndIf
EndEvent
