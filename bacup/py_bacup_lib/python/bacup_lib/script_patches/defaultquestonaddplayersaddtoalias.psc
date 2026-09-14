Event OnQuestInit()
    Actor playerRef = Game.GetPlayer()
    If AliasToForceTo != None
        AliasToForceTo.ForceRefTo(playerRef)
    EndIf
    If CollectionAliasToAddTo != None && CollectionAliasToAddTo.Find(playerRef) < 0
        CollectionAliasToAddTo.AddRef(playerRef)
    EndIf
EndEvent
