; POI061 House of Scares has no stage fragments: the player alias owns objective ObjectiveToSet ("Kill a Wendigo
; while wearing a clown costume"). The patched HouseOfScaresScript detects the kill and sets 1000, which completes the
; objective and the quest.
Event OnAliasInit()
    Quest owner = GetOwningQuest()
    RegisterForRemoteEvent(owner, "OnStageSet")
    If !owner.IsStageDone(1000)
        owner.SetObjectiveDisplayed(ObjectiveToSet, True)
    EndIf
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    If auiStageID != 1000
        Return
    EndIf
    UnregisterForRemoteEvent(akSender, "OnStageSet")
    akSender.SetObjectiveCompleted(ObjectiveToSet, True)
    akSender.CompleteQuest()
    ; B21:QuestRewards pays on the same OnStageSet; let it dispatch before shutdown.
    Utility.Wait(1.0)
    akSender.Stop()
EndEvent
