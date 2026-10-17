; A row's optional requirements (faction, actor value, completed quest,
; keyword) gate it; with UseORConditions any one set requirement suffices.
Bool Function RowRequirementsMet(ValueData row, Actor akPlayer)
    Int requirementCount = 0
    Int metCount = 0
    If row.RequiredFaction != None
        requirementCount += 1
        If akPlayer.IsInFaction(row.RequiredFaction)
            metCount += 1
        EndIf
    EndIf
    If row.RequiredActorValue != None
        requirementCount += 1
        If akPlayer.GetValue(row.RequiredActorValue) >= row.RequiredActorValueValue
            metCount += 1
        EndIf
    EndIf
    If row.CompletedQuest != None
        requirementCount += 1
        If row.CompletedQuest.IsCompleted()
            metCount += 1
        EndIf
    EndIf
    If row.RequiredKeyword != None
        requirementCount += 1
        If akPlayer.HasKeyword(row.RequiredKeyword)
            metCount += 1
        EndIf
    EndIf
    If requirementCount == 0
        Return True
    ElseIf row.UseORConditions
        Return metCount > 0
    EndIf
    Return metCount == requirementCount
EndFunction

; FO4 gates each taught recipe on its B21_PlanLearned_* global; conversion binds
; LearnedGlobals one-to-one with ValueSet rows, None for an ungated recipe.
Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If ValueSet == None || LearnedGlobals == None || playerRef == None
        Return
    EndIf
    Bool learnedAny = False
    Int index = 0
    While index < ValueSet.Length && index < LearnedGlobals.Length
        ValueData row = ValueSet[index]
        GlobalVariable learned = LearnedGlobals[index]
        If row != None && learned != None && row.iMenuItemTarget == auiMenuItemID && (row.TargetTerminal == None || row.TargetTerminal == Self)
            If learned.GetValue() < 1.0 && RowRequirementsMet(row, playerRef)
                learned.SetValue(1.0)
                learnedAny = True
            EndIf
        EndIf
        index += 1
    EndWhile
    If learnedAny
        Debug.Notification("Recipe learned.")
    EndIf
EndEvent
