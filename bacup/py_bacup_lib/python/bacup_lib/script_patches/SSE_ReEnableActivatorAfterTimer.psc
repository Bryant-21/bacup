Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    If SSE_BigBloom != None
        SSE_BigBloom.SetStage(QuestStage)
    EndIf
    Disable()
    StartTimer(FlowerResetTimer as Float, resetTimerID)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == resetTimerID
        Enable()
    EndIf
EndEvent
