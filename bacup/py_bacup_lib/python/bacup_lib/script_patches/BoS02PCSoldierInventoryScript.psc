Event OnAliasInit()
    If pBoS02SoldierCertificate != None
        AddInventoryEventFilter(pBoS02SoldierCertificate)
    EndIf
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || owningQuest.GetStage() < 400 || owningQuest.GetStage() >= 500
        Return
    EndIf
    Actor playerRef = GetActorReference()
    Bool hasCertificate = playerRef != None && pBoS02SoldierCertificate != None && playerRef.GetItemCount(pBoS02SoldierCertificate) > 0
    Bool trainingComplete = pEN05_Basic != None && pEN05_Basic.IsCompleted()
    If hasCertificate || trainingComplete
        owningQuest.SetStage(500)
    EndIf
EndEvent

Event OnItemAdded(Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || owningQuest.GetStage() < 400 || owningQuest.GetStage() >= 500
        Return
    EndIf
    If akBaseItem == pBoS02SoldierCertificate && aiItemCount > 0
        owningQuest.SetStage(500)
    EndIf
EndEvent
