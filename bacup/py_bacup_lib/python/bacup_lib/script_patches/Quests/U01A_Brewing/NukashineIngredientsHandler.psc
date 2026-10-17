; From StartStage to EndStage, counts each ingredient (components across all junk, or the item itself), shows
; its <Variable=TextVar> count, completes its objective at NumNeeded, then moves CurrentObjective on to
; NextObjective (craft it at a Brewing Station).
Event OnAliasInit()
    SelfRef = GetReference()
    OwningQuest = GetOwningQuest()
    NumItemsAcquired = 0
    spinlock = False
    RegisterForRemoteEvent(OwningQuest, "OnStageSet")
    If OwningQuest.IsStageDone(StartStage) && !OwningQuest.IsStageDone(EndStage)
        StartTimer(1.0, 1)
    EndIf
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    If auiStageID == StartStage
        StartTimer(1.0, 1)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If OwningQuest == None || !OwningQuest.IsRunning() || OwningQuest.IsStageDone(EndStage)
        Return
    EndIf
    UpdateIngredients()
    StartTimer(2.0, 1)
EndEvent

Function UpdateIngredients()
    If spinlock || Ingredients == None
        Return
    EndIf
    spinlock = True
    SelfRef = GetReference()
    B21:QuestVariables questVariables = OwningQuest as B21:QuestVariables
    Int acquired = 0
    Int index = 0
    While index < Ingredients.Length
        IngredientStruct entry = Ingredients[index]
        If entry != None
            Int needed = entry.NumNeeded
            If needed < 1
                needed = 1
            EndIf
            Int have = 0
            If SelfRef != None
                If entry.ComponentForm != None
                    have = SelfRef.GetComponentCount(entry.ComponentForm)
                ElseIf entry.ItemForm != None
                    have = SelfRef.GetItemCount(entry.ItemForm)
                EndIf
            EndIf
            If have > needed
                have = needed
            EndIf
            If questVariables != None && entry.TextVar != ""
                questVariables.SetVariable(entry.TextVar, have as Float)
            EndIf
            entry.Acquired = have >= needed
            If entry.Acquired
                acquired += 1
            EndIf
            If entry.Objective > 0
                OwningQuest.SetObjectiveCompleted(entry.Objective, entry.Acquired)
            EndIf
        EndIf
        index += 1
    EndWhile
    NumItemsAcquired = acquired
    Bool allAcquired = acquired >= Ingredients.Length
    OwningQuest.SetObjectiveCompleted(CurrentObjective, allAcquired)
    OwningQuest.SetObjectiveDisplayed(NextObjective, allAcquired)
    spinlock = False
EndFunction
