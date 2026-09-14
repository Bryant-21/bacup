Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
    Quest owningQuest = GetOwningQuest()
    If owningQuest != None && owningQuest.IsStageDone(70) && !owningQuest.IsStageDone(100)
        owningQuest.SetStage(100)
    EndIf
EndEvent
