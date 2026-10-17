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
    If !myQuest.IsObjectiveDisplayed(EggsObjective) || myQuest.IsObjectiveCompleted(EggsObjective) || myQuest.IsStageDone(StageToSet)
        Return
    EndIf
    Remaining = EggsNeeded - Count
    Taken = myPlayer.GetItemCount(Eggs)
    If Taken > Remaining
        Taken = Remaining
    EndIf
    If Taken <= 0
        Return
    EndIf
    myPlayer.RemoveItem(Eggs, Taken)
    Count += Taken
    B21:QuestVariables questVariables = myQuest as B21:QuestVariables
    If questVariables != None
        questVariables.SetVariable("Eggs", Count)
    EndIf
    If Count >= EggsNeeded
        myQuest.SetStage(StageToSet)
    EndIf
EndEvent
