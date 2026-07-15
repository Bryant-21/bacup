Event OnTriggerEnter(ObjectReference akActionRef)
    If bOnLeave == 0
        TrySetStage(akActionRef)
    EndIf
EndEvent

Event OnTriggerLeave(ObjectReference akActionRef)
    If bOnLeave != 0
        TrySetStage(akActionRef)
    EndIf
EndEvent

Function TrySetStage(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || pBoSQuest == None
        Return
    EndIf

    If pPreReqStage <= 0 || pBoSQuest.IsStageDone(pPreReqStage)
        pBoSQuest.SetStage(pStageToSet)
    EndIf
EndFunction
