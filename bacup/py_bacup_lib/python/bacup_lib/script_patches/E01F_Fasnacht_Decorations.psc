; MessageSent is the decoration menu: buttons 0-2 hang DecorItem01-03 (disabled AllDecorMarkers refs), button 3 is "Do Nothing".
Event OnActivate(ObjectReference akActionRef)
    myQuest = GetOwningQuest()
    If akActionRef != Game.GetPlayer() || myQuest == None || MessageSent == None
        Return
    EndIf
    If !myQuest.IsObjectiveDisplayed(70) || myQuest.IsObjectiveCompleted(70) || myQuest.IsStageDone(StageToSet)
        Return
    EndIf
    Int choice = MessageSent.Show()
    ReferenceAlias chosen = None
    If choice == 0
        chosen = DecorItem01
    ElseIf choice == 1
        chosen = DecorItem02
    ElseIf choice == 2
        chosen = DecorItem03
    EndIf
    If chosen == None || chosen.GetReference() == None
        Return
    EndIf
    chosen.GetReference().Enable()
    myQuest.SetStage(StageToSet)
EndEvent
