; The flower's event alias hands out the flower; this base script only regrows it. QuestStage is the basket's
; "all collected" stage for this flower type (500/550/600), after which the meadow stops regrowing it.
Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    Disable()
    StartTimer(FlowerResetTimer as Float, resetTimerID)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != resetTimerID
        Return
    EndIf
    If SSE_BigBloom != None && SSE_BigBloom.IsRunning() && SSE_BigBloom.IsStageDone(QuestStage)
        Return
    EndIf
    Enable()
EndEvent
