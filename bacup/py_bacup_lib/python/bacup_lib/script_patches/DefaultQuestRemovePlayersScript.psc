Event OnQuestInit()
    cleanupLock = False
    cleanupDone = False
    Utility.Wait(0.1)
    If IsRunning()
        FillPlayerAliases()
    EndIf
EndEvent

Event OnQuestShutdown()
    CleanUpQuestItems()
EndEvent

Function FillPlayerAliases()
    Actor player = Game.GetPlayer()
    Int index = 0
    While playerAliases && index < playerAliases.Length
        If playerAliases[index]
            playerAliases[index].ForceRefIfEmpty(player)
        EndIf
        index = index + 1
    EndWhile

    index = 0
    While playerCollections && index < playerCollections.Length
        If playerCollections[index] && playerCollections[index].Find(player) < 0
            playerCollections[index].AddRef(player)
        EndIf
        index = index + 1
    EndWhile
EndFunction

Function CleanUpQuestItems()
    If cleanupLock || cleanupDone
        Return
    EndIf
    cleanupLock = True

    Int index = 0
    While QuestItemsToCleanUpArray && index < QuestItemsToCleanUpArray.Length
        If QuestItemsToCleanUpArray[index].DoNotCleanUpOnStage < 0 || !IsStageDone(QuestItemsToCleanUpArray[index].DoNotCleanUpOnStage)
            If QuestItemsToCleanUpArray[index].QuestItemToCleanUp
                RemoveQuestReference(QuestItemsToCleanUpArray[index].QuestItemToCleanUp.GetReference())
            EndIf
            RefCollectionAlias collection = QuestItemsToCleanUpArray[index].QuestItemCollectionToCleanUp
            If collection
                Int itemIndex = 0
                While itemIndex < collection.GetCount()
                    RemoveQuestReference(collection.GetAt(itemIndex))
                    itemIndex = itemIndex + 1
                EndWhile
            EndIf
        EndIf
        index = index + 1
    EndWhile

    cleanupDone = True
    cleanupLock = False
EndFunction

Function RemoveQuestReference(ObjectReference itemRef)
    If !itemRef
        Return
    EndIf
    Actor player = Game.GetPlayer()
    If itemRef.GetContainer() == player
        player.RemoveItem(itemRef, 1, True)
    EndIf
EndFunction
