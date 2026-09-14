Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
    If !UseOnDyingInstead
        RemoveDeadReference(akSenderRef)
    EndIf
EndEvent

Event OnDying(ObjectReference akSenderRef, Actor akKiller)
    If UseOnDyingInstead
        RemoveDeadReference(akSenderRef)
    EndIf
EndEvent

Function RemoveDeadReference(ObjectReference akSenderRef)
    If akSenderRef == None
        Return
    EndIf

    DefaultQuestEncounterWaveScript encounterQuest = GetOwningQuest() as DefaultQuestEncounterWaveScript
    If encounterQuest != None
        encounterQuest.HandleLocalEncounterActorDeath(akSenderRef)
    EndIf

    RemoveRef(akSenderRef)
    Quest owningQuest = GetOwningQuest()
    If owningQuest != None && StageToSetOnEmpty >= 0 && GetCount() <= 0 && !owningQuest.IsStageDone(StageToSetOnEmpty)
        owningQuest.SetStage(StageToSetOnEmpty)
    EndIf
EndFunction
