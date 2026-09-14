Event OnTriggerEnter(ObjectReference akActionRef)
    Debug.Trace("[B21 BoSZ04] Start trigger entered ref=" + akActionRef as String + " quest=" + QuestToStart as String + " keyword=" + QuestKeyword as String, 0)
    If akActionRef != Game.GetPlayer()
        Debug.Trace("[B21 BoSZ04] Start trigger ignored non-player ref=" + akActionRef as String, 0)
        Return
    EndIf
    If QuestToStart == None
        Debug.Trace("[B21 BoSZ04] Start trigger has no bound quest", 0)
        Return
    EndIf
    Debug.Trace("[B21 BoSZ04] Start trigger quest state running=" + QuestToStart.IsRunning() as String + " completed=" + QuestToStart.IsCompleted() as String + " stage=" + QuestToStart.GetStage() as String, 0)
    If QuestToStart.IsRunning() || QuestToStart.IsCompleted()
        Debug.Trace("[B21 BoSZ04] Start trigger skipped existing quest state", 0)
        Return
    EndIf

    Bool startedFromStory = False
    If QuestKeyword != None
        startedFromStory = QuestKeyword.SendStoryEventAndWait(None, akActionRef)
        Debug.Trace("[B21 BoSZ04] Start trigger story result=" + startedFromStory as String + " running=" + QuestToStart.IsRunning() as String + " stage=" + QuestToStart.GetStage() as String, 0)
    EndIf
    If !startedFromStory && !QuestToStart.IsRunning()
        Debug.Trace("[B21 BoSZ04] Start trigger falling back to direct quest start", 0)
        QuestToStart.Start()
        Debug.Trace("[B21 BoSZ04] Start trigger direct result running=" + QuestToStart.IsRunning() as String + " stage=" + QuestToStart.GetStage() as String, 0)
    EndIf
EndEvent
