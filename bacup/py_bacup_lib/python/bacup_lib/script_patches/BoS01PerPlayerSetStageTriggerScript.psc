Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer() && BaseQuest != None
        BaseQuest.SetStage(stage)
    EndIf
EndEvent
