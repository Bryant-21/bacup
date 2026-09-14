Event OnTriggerLeave(ObjectReference akActionRef)
	Bool playerOnly = PlayerTriggerType >= 0
	TryToSetStage(PlayerCheckOverride = playerOnly, RefToCheck = akActionRef, ReferenceArray = TriggeredByReferences, AliasArray = TriggeredByAliases, FactionArray = TriggeredByFactions)
EndEvent
