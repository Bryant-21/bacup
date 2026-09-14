Event OnTriggerEnter(ObjectReference akActionRef)
	Bool playerOnly = PlayerTriggerType >= 0 || PlayerTriggerOnly
	TryToSetStage(PlayerCheckOverride = playerOnly, RefToCheck = akActionRef, ReferenceArray = TriggeredByReferences, AliasArray = TriggeredByAliases, FactionArray = TriggeredByFactions)
EndEvent
