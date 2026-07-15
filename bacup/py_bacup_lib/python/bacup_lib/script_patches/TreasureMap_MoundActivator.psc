Event OnActivate(ObjectReference akActionRef)
    PlayAnimation("JumpState01")
    PlayAnimation("Play01")

    Actor activatingActor = akActionRef as Actor
    If activatingActor == None || TreasureMap == None || GoodItem == None
        Return
    EndIf
    If activatingActor.GetItemCount(TreasureMap) < 1
        Return
    EndIf

    activatingActor.RemoveItem(TreasureMap, 1, True)
    activatingActor.AddItem(GoodItem)
    If TreasureMapsFound != None
        activatingActor.ModValue(TreasureMapsFound, 1.0)
    EndIf
    If QuestTape != None && activatingActor.GetItemCount(QuestTape) == 0
        activatingActor.AddItem(QuestTape)
    EndIf
EndEvent
