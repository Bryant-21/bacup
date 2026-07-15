; Shelter workshop containers carry the normal FO4 workshop keywords, so the
; native workshop-mode entry point is a safe local replacement for server CAMP
; ownership/instance handling.

Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        StartWorkshop(True)
    EndIf
EndEvent
