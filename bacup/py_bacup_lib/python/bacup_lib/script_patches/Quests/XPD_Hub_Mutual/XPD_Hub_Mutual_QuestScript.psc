Event OnQuestInit()
    myPlayer = PlayerAlias.GetActorReference()
EndEvent

Function SelectDonation()
    If myPlayer == None || Items == None || Items.Length == 0
        Return
    EndIf

    ChosenItem = Items[Utility.RandomInt(0, Items.Length - 1)]
    selectedDonation = ChosenItem.ItemToDonate
    numToDonate = ChosenItem.Quantity
    If numToDonate <= 0
        numToDonate = StandardQuantity
    EndIf
    If selectedDonation == None || numToDonate <= 0
        Return
    EndIf

EndFunction

Function DisplayDonationObjectives()
    If selectedDonation == None || numToDonate <= 0
        Return
    EndIf

    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(ChosenItem.StageToSet)
    If !IsStageDone(ChosenItem.StageToSet)
        SetStage(ChosenItem.StageToSet)
    EndIf
EndFunction

Function CompleteDonation()
    If myPlayer == None || selectedDonation == None || numToDonate <= 0
        Return
    EndIf
    If myPlayer.GetItemCount(selectedDonation) < numToDonate
        Return
    EndIf

    myPlayer.RemoveItem(selectedDonation, numToDonate, true)
    SetObjectiveCompleted(ChosenItem.StageToSet)
    SetObjectiveCompleted(ChosenItem.StageToSet + 10)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 100
        SelectDonation()
        SetObjectiveDisplayed(10)
        If !IsStageDone(200)
            SetStage(200)
        EndIf
    ElseIf auiStageID == 250
        DisplayDonationObjectives()
    ElseIf auiStageID == 300
        SetObjectiveDisplayed(ChosenItem.StageToSet + 10)
        If !IsStageDone(ChosenItem.StageToSet + 50)
            SetStage(ChosenItem.StageToSet + 50)
        EndIf
    ElseIf auiStageID == 350
        SetObjectiveCompleted(ChosenItem.StageToSet + 10)
    ElseIf auiStageID == CompletionStage
        CompleteDonation()
    ElseIf auiStageID == 9000
        CompleteAllObjectives()
    EndIf
EndEvent
