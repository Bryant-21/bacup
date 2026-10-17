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
    If !myQuest.IsObjectiveDisplayed(MeatObjective) || myQuest.IsObjectiveCompleted(MeatObjective) || myQuest.IsStageDone(StageToSet)
        Return
    EndIf
    Remaining = MeatNeeded - Count
    Taken = myPlayer.GetItemCount(CritterMeat)
    If Taken > Remaining
        Taken = Remaining
    EndIf
    If Taken <= 0
        Return
    EndIf
    myPlayer.RemoveItem(CritterMeat, Taken)
    Count += Taken
    B21:QuestVariables questVariables = myQuest as B21:QuestVariables
    If questVariables != None
        questVariables.SetVariable("Meat", Count)
    EndIf
    If Count >= MeatNeeded
        myQuest.SetStage(StageToSet)
    EndIf
EndEvent
