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
    If !myQuest.IsObjectiveDisplayed(WoodObjective) || myQuest.IsObjectiveCompleted(WoodObjective) || myQuest.IsStageDone(StageToSet)
        Return
    EndIf
    Remaining = WoodNeeded - Count
    Taken = myPlayer.GetItemCount(Wood)
    If Taken > Remaining
        Taken = Remaining
    EndIf
    If Taken <= 0
        Return
    EndIf
    myPlayer.RemoveItem(Wood, Taken)
    Count += Taken
    B21:QuestVariables questVariables = myQuest as B21:QuestVariables
    If questVariables != None
        questVariables.SetVariable("Wood", Count)
    EndIf
    If Count >= WoodNeeded
        myQuest.SetStage(StageToSet)
    EndIf
EndEvent
