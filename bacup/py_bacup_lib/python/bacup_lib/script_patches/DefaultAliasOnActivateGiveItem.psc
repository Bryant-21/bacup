Event OnActivate(ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If bBusy || akActionRef != playerRef || ItemToGive == None
        Return
    EndIf

    If RequiredActorValue != None && playerRef.GetValue(RequiredActorValue) != RequiredActorValueValue
        Return
    EndIf

    bBusy = True
    GoToState("done")
    playerRef.AddItem(ItemToGive, iItemCountToGive, !ShowItemAddMessage)

    ObjectReference activatedRef = GetReference()
    If activatedRef != None
        If ReferenceAliasToAddTo != None
            ReferenceAliasToAddTo.ForceRefTo(activatedRef)
        EndIf
        If ReferenceCollectionAliasToAddTo != None
            ReferenceCollectionAliasToAddTo.AddRef(activatedRef)
        EndIf
        If bDisableAfterGive
            activatedRef.Disable()
        EndIf
    EndIf

    TryToSetStage(PlayerCheckOverride = PlayerActivateOnly, RefToCheck = akActionRef, ReferenceArray = ActivatedByReferences, AliasArray = ActivatedByAliases, FactionArray = ActivatedByFactions)
EndEvent
