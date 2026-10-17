Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
EndEvent

Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
    If akSenderRef == None || !MischiefOpen()
        Return
    EndIf
    ScoreMischief(akSenderRef, Game.GetPlayer(), 1.0)
EndEvent
