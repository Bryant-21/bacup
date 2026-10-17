; RE_SceneTS07 bandit Handy hold-up. While held up (100, before the bag stage 300) an item the player drops is
; gathered into a FO76-style brown bag (BrownBagContainer) at the player's feet, filling BrownBag and setting 300;
; the robot tells them to scram and takes the bag once they leave (400).
Event OnAliasInit()
    RemoveAllInventoryEventFilters()
    AddInventoryEventFilter(None)
EndEvent

Event OnItemRemoved(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    Quest owner = GetOwningQuest()
    If akDestContainer != None || owner == None || !owner.IsStageDone(100) || owner.IsStageDone(300) || owner.IsStageDone(200)
        Return
    EndIf
    Actor player = GetActorReference()
    If player == None || BrownBagContainer == None
        Return
    EndIf
    ObjectReference bag = None
    If BrownBag != None
        bag = BrownBag.GetReference()
    EndIf
    If bag == None
        bag = player.PlaceAtMe(BrownBagContainer)
        If BrownBag != None
            BrownBag.ForceRefTo(bag)
        EndIf
    EndIf
    If akItemReference != None
        bag.AddItem(akItemReference)
    EndIf
    owner.SetStage(300)
EndEvent
