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
        CancelTimer(1)
        StartTimer(3.0, 1)
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
        CancelTimer(1)
        CompleteAllObjectives()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != 1 || !IsRunning()
        Return
    EndIf
    AdvanceDonation()
    If IsRunning() && !IsStageDone(9000)
        StartTimer(3.0, 1)
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(1)
EndEvent

ObjectReference Function GetSophie()
    ReferenceAlias sophieAlias = GetAlias(7) as ReferenceAlias
    ObjectReference sophie = None
    If sophieAlias != None
        sophie = sophieAlias.GetReference()
    EndIf
    If sophie == None
        ReferenceAlias markerAlias = GetAlias(15) as ReferenceAlias
        If markerAlias != None
            sophie = markerAlias.GetReference()
        EndIf
    EndIf
    Return sophie
EndFunction

; Mutual_Aid_Main_Scene is dead in the conversion, so standing with Sophie stands
; in for asking what she needs, for telling her the player is short, and for
; handing the donation over.
Function AdvanceDonation()
    If myPlayer == None
        myPlayer = PlayerAlias.GetActorReference()
    EndIf
    ObjectReference sophie = GetSophie()
    If myPlayer == None || sophie == None || myPlayer.GetDistance(sophie) > 384.0
        Return
    EndIf

    If !IsStageDone(250)
        SetStage(250)
        Return
    EndIf
    If selectedDonation == None || numToDonate <= 0
        Return
    EndIf

    If myPlayer.GetItemCount(selectedDonation) >= numToDonate
        If !IsStageDone(CompletionStage)
            SetStage(CompletionStage)
        EndIf
    ElseIf !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction
