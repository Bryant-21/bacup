Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
    TryToSetStage(TriggeredRef = akSenderRef, setStageOnSingleTrigger = setStageWhenAnyRefDies)
EndEvent
