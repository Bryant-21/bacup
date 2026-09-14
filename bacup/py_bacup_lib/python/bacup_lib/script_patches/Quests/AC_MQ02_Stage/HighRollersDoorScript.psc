Event OnAliasInit()
    OwningQuest = GetOwningQuest()
EndEvent

Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    If OwningQuest == None
        OwningQuest = GetOwningQuest()
    EndIf
    ObjectReference doorRef = GetReference()
    If OwningQuest == None || doorRef == None
        Return
    EndIf
    If !OwningQuest.IsStageDone(Stage_PassedCharismaCheck) && !OwningQuest.IsStageDone(Stage_BrokeInMainDoor)
        OwningQuest.SetStage(Stage_BrokeInMainDoor)
    EndIf
    doorRef.BlockActivation(False)
    doorRef.Lock(False)
    doorRef.Activate(akActionRef, True)
EndEvent
