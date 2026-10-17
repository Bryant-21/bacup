; Tracks the player's Nukashine: the party bottle (350), drinking it (400), crafting the fermenting brew once the
; recipe is known (800) and holding finished Fresh or Vintage Nukashine for Biv (900).
Event OnAliasInit()
    SelfRef = GetReference()
    OwningQuest = GetOwningQuest()
    AddInventoryEventFilter(NukashineFresh)
    AddInventoryEventFilter(NukashineVintage)
    AddInventoryEventFilter(NukashineFerm)
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If OwningQuest == None
        OwningQuest = GetOwningQuest()
    EndIf
    If akBaseItem == NukashineFerm
        If OwningQuest.IsStageDone(Stage_LearnedRecipe) && !OwningQuest.IsStageDone(Stage_CraftedFermenting)
            OwningQuest.SetStage(Stage_CraftedFermenting)
        EndIf
    ElseIf akBaseItem == NukashineFresh || akBaseItem == NukashineVintage
        HandleFinishedNukashine()
    EndIf
EndEvent

Function HandleFinishedNukashine()
    If OwningQuest.IsStageDone(Stage_LearnedRecipe)
        If !OwningQuest.IsStageDone(Stage_AcquiredNukashineToTurnIn)
            OwningQuest.SetStage(Stage_AcquiredNukashineToTurnIn)
        EndIf
    ElseIf OwningQuest.IsStageDone(Stage_EnteredPiHouse) && !OwningQuest.IsStageDone(Stage_AcquiredNukashineAtPiHouse)
        OwningQuest.SetStage(Stage_AcquiredNukashineAtPiHouse)
    EndIf
EndFunction

Event OnItemEquipped(Form akBaseObject, ObjectReference akReference)
    If akBaseObject != NukashineFresh && akBaseObject != NukashineVintage
        Return
    EndIf
    If OwningQuest == None
        OwningQuest = GetOwningQuest()
    EndIf
    If OwningQuest.IsStageDone(Stage_EnteredPiHouse) && !OwningQuest.IsStageDone(Stage_DrankNukashine)
        OwningQuest.SetStage(Stage_DrankNukashine)
    EndIf
EndEvent
