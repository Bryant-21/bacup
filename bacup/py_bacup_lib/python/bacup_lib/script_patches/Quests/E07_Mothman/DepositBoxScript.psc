; Blood troughs (ItemToLookFor) take albino radstag blood toward Goal_BloodAdded; the special deposit trigger
; (FoodToLookFor, a Perfect Mothman Egg) sets the bonus SpecialConditionStage.
Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
    Actor player = Game.GetPlayer()
    OwningQuest = GetOwningQuest()
    QuestScript = OwningQuest as Quests:E07_Mothman:QuestScript
    If akActionRef != player || QuestScript == None
        Return
    EndIf

    If FoodToLookFor != None
        If OwningQuest.IsStageDone(SpecialConditionStage)
            Return
        EndIf
        If player.GetItemCount(FoodToLookFor) <= 0
            ShowNoItem()
            Return
        EndIf
        player.RemoveItem(FoodToLookFor, 1)
        OwningQuest.SetStage(SpecialConditionStage)
        Return
    EndIf

    If ItemToLookFor == None || !OwningQuest.IsStageDone(3000) || OwningQuest.IsStageDone(QuestScript.Stage_BloodComplete)
        Return
    EndIf
    Int needed = QuestScript.BloodStillNeeded()
    Int taken = player.GetItemCount(ItemToLookFor)
    If taken > needed
        taken = needed
    EndIf
    If taken <= 0
        ShowNoItem()
        Return
    EndIf
    player.RemoveItem(ItemToLookFor, taken)
    QuestScript.BloodAdded(taken)
EndEvent

Function ShowNoItem()
    If E07_Mothman_Message_NoItem != None
        E07_Mothman_Message_NoItem.Show()
    EndIf
EndFunction
