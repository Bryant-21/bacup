; Only ValidSteins (Old Fasnacht Steins) count toward TotalCount; the other SteinsFormList steins are
; accepted for analysis. Each hand-in prints a Good, Mixed or Bad analysis at the Printer.
Event OnAliasInit()
    Count = 0
EndEvent

Event OnActivate(ObjectReference akActionRef)
    myPlayer = Game.GetPlayer()
    myQuest = GetOwningQuest()
    If akActionRef != myPlayer || myQuest == None || ValidSteins == None
        Return
    EndIf
    If !myQuest.IsObjectiveDisplayed(SteinCounterObjective) || myQuest.IsObjectiveCompleted(SteinCounterObjective) || myQuest.IsStageDone(HelpedHistorianStage)
        Return
    EndIf

    Int validTaken = TakeSteins(ValidSteins, TotalCount - Count)
    Int otherTaken = 0
    FormList allSteins = SteinsFormList as FormList
    If allSteins != None
        Int index = 0
        While index < allSteins.GetSize()
            Form stein = allSteins.GetAt(index)
            If stein != None && stein != ValidSteins
                otherTaken += TakeSteins(stein, myPlayer.GetItemCount(stein))
            EndIf
            index += 1
        EndWhile
    EndIf
    If validTaken + otherTaken <= 0
        Return
    EndIf

    PrintAnalysis(validTaken, otherTaken)
    Count += validTaken
    B21:QuestVariables questVariables = myQuest as B21:QuestVariables
    If questVariables != None
        questVariables.SetVariable("Steins", Count)
    EndIf
    If Count >= TotalCount
        If FinalReport != None
            FinalReport.Enable()
        EndIf
        myQuest.SetStage(HelpedHistorianStage)
    EndIf
EndEvent

Int Function TakeSteins(Form akStein, Int aiMax)
    Int taken = myPlayer.GetItemCount(akStein)
    If taken > aiMax
        taken = aiMax
    EndIf
    If taken > 0
        myPlayer.RemoveItem(akStein, taken)
    EndIf
    Return taken
EndFunction

Function PrintAnalysis(Int aiValid, Int aiOther)
    Book analysis = Analysis_Mixed
    If aiOther == 0
        analysis = Analysis_Good
    ElseIf aiValid == 0
        analysis = Analysis_Bad
    EndIf
    If analysis == None
        Return
    EndIf
    If Printer != None
        Printer.PlaceAtMe(analysis)
    Else
        myPlayer.AddItem(analysis)
    EndIf
EndFunction
