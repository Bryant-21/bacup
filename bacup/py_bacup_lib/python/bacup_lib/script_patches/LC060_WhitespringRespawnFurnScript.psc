; Whitespring ambush fabricator: each use locks the furniture for RespawnFurnitureUseDelay seconds before the next
; robot can come out of it.
Event OnActivate(ObjectReference akActionRef)
    GotoState("locked")
    BlockActivation(True)
    StartTimer(RespawnFurnitureUseDelay as Float)
EndEvent

Event OnTimer(Int aiTimerID)
    BlockActivation(False)
    GotoState("available")
EndEvent

State locked
    Event OnActivate(ObjectReference akActionRef)
    EndEvent
EndState
