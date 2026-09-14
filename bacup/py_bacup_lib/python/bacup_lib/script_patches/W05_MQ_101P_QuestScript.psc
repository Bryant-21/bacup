; The three ToTW aliases ship with a NULL forced reference and are Optional, so
; they can only be populated by W05_MQ_101P_A once it has created those actors.
Function FillTopOfTheWorldAliases(ObjectReference akMeg, ObjectReference akRaiderA, ObjectReference akRaiderB)
    If MegAtTopOfTheWorld && akMeg
        MegAtTopOfTheWorld.ForceRefTo(akMeg)
    EndIf
    If RaiderAAtTopOfTheWorld && akRaiderA
        RaiderAAtTopOfTheWorld.ForceRefTo(akRaiderA)
    EndIf
    If RaiderBAtTopOfTheWorld && akRaiderB
        RaiderBAtTopOfTheWorld.ForceRefTo(akRaiderB)
    EndIf
EndFunction
