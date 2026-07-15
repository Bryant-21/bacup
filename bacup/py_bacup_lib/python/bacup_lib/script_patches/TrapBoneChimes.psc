; Extend the vanilla FO4 can-chime trigger, then cast the only additional bound
; payload on an actor that disturbs the chimes.

Event OnTriggerEnter(ObjectReference akActionRef)
    Parent.OnTriggerEnter(akActionRef)
    If DiseaseChanceSpell != None && akActionRef as Actor != None
        DiseaseChanceSpell.Cast(Self, akActionRef)
    EndIf
EndEvent
