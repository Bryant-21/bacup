Event OnAliasInit()
    Count = 0
    Taken = 0
EndEvent

Event OnActivate(ObjectReference akActionRef)
    myPlayer = Game.GetPlayer()
    myQuest = GetOwningQuest()
    If akActionRef != myPlayer || myQuest == None
        Return
    EndIf
    If !myQuest.IsObjectiveDisplayed(WaxObjective) || myQuest.IsObjectiveCompleted(WaxObjective) || myQuest.IsStageDone(StageToSet)
        Return
    EndIf
    Remaining = WaxNeeded - Count
    Taken = myPlayer.GetItemCount(Beeswax)
    If Taken > Remaining
        Taken = Remaining
    EndIf
    If Taken <= 0
        Return
    EndIf
    myPlayer.RemoveItem(Beeswax, Taken)
    Count += Taken
    B21:QuestVariables questVariables = myQuest as B21:QuestVariables
    If questVariables != None
        questVariables.SetVariable("Beeswax", Count)
    EndIf
    If Count >= WaxNeeded
        myQuest.SetStage(StageToSet)
    EndIf
EndEvent
