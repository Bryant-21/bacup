Function RemoveQuestItemFromPlayer(ObjectReference akItemReference)
    Actor playerRef = Game.GetPlayer()
    If akItemReference != None && akItemReference.GetContainer() == playerRef
        playerRef.RemoveItem(akItemReference, 1, True)
    EndIf
EndFunction

Event OnQuestShutdown()
    If QuestItemsToCleanUpArray == None
        Return
    EndIf

    Int datumIndex = 0
    While datumIndex < QuestItemsToCleanUpArray.Length
        QuestItemsToCleanUp cleanupDatum = QuestItemsToCleanUpArray[datumIndex]
        If cleanupDatum.DoNotCleanUpOnStage < 0 || !IsStageDone(cleanupDatum.DoNotCleanUpOnStage)
            If cleanupDatum.QuestItemToCleanUp != None
                RemoveQuestItemFromPlayer(cleanupDatum.QuestItemToCleanUp.GetReference())
            EndIf
            If cleanupDatum.QuestItemCollectionToCleanUp != None
                Int itemIndex = cleanupDatum.QuestItemCollectionToCleanUp.GetCount() - 1
                While itemIndex >= 0
                    RemoveQuestItemFromPlayer(cleanupDatum.QuestItemCollectionToCleanUp.GetAt(itemIndex))
                    itemIndex -= 1
                EndWhile
            EndIf
        EndIf
        datumIndex += 1
    EndWhile
EndEvent
